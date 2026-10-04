//! Semantic validation against fixture declarations, never indexed results.
use super::fixture::{ExtendedFixture, Record, Shape};
use super::*;
use std::cmp::Ordering;

#[derive(Clone, Copy, Debug)]
pub(super) struct Filter {
    pub(super) files: bool,
    pub(super) dirs: bool,
    pub(super) ignore_enabled: bool,
    pub(super) ignore_case: bool,
}
pub(super) struct ExtendedOracle {
    source: Source,
    all: Vec<Record>,
    all_by_path: HashMap<PathBuf, Record>,
    visible: HashMap<PathBuf, Record>,
    scores: HashMap<PathBuf, f64>,
    score_top: Vec<(PathBuf, f64)>,
    globally_sorted: Vec<(PathBuf, f64)>,
    required_above_cutoff: HashSet<PathBuf>,
    link_paths: HashSet<PathBuf>,
    cutoff: f64,
    count: usize,
    limit: usize,
    mode: ResultSortMode,
    scope: ResultSortScope,
    empty: bool,
    identity: String,
}
impl ExtendedOracle {
    pub(super) fn new(
        fixture: &ExtendedFixture,
        source: Source,
        filter: Filter,
        query: &str,
        mode: ResultSortMode,
        scope: ResultSortScope,
        limit: usize,
    ) -> Self {
        assert!(limit > 0);
        assert!(
            matches!(query, "" | "item" | "needle"),
            "extended fixture query needs an independent membership rule"
        );
        assert!(
            !matches!(
                mode,
                ResultSortMode::CreatedAsc | ResultSortMode::CreatedDesc
            ),
            "creation time is not a fixed fixture field"
        );
        assert!(
            !(source == Source::Walker
                && matches!(fixture.shape, Shape::NestedEarly | Shape::NestedLate)),
            "H1 final hierarchy oracle is FileList-only"
        );
        let logical = if source == Source::FileList && fixture.shape == Shape::InternalLinks {
            &fixture.records
        } else {
            &fixture.expected
        };
        let all = logical
            .iter()
            .filter(|r| if r.is_dir { filter.dirs } else { filter.files })
            .cloned()
            .collect::<Vec<_>>();
        let visible_records = all
            .iter()
            .filter(|r| {
                let relative = r
                    .path
                    .strip_prefix(&fixture.root)
                    .unwrap()
                    .to_string_lossy();
                !filter.ignore_enabled
                    || !r.ignored
                    || !(relative.contains("SKIP")
                        || (filter.ignore_case && relative.contains("skip")))
            })
            .cloned()
            .collect::<Vec<_>>();
        // For these deliberately simple patterns, independent literal membership
        // must agree with the uncached scorer. No product filter/sort is reused.
        let count = visible_records
            .iter()
            .filter(|r| {
                query.is_empty()
                    || r.path
                        .strip_prefix(&fixture.root)
                        .unwrap()
                        .to_string_lossy()
                        .contains(query)
            })
            .count();
        let entries = Arc::new(
            visible_records
                .iter()
                .map(|r| {
                    if r.is_dir {
                        Entry::dir(r.path.clone())
                    } else {
                        Entry::file(r.path.clone())
                    }
                })
                .collect(),
        );
        let (scored, error) = crate::search::rank_search_results_uncached(
            &entries,
            query,
            &fixture.root,
            visible_records.len().max(1),
            false,
            filter.ignore_case,
            true,
            ResultSortMode::Score,
            ResultSortScope::ShownResults,
        );
        assert!(error.is_none());
        assert_eq!(
            scored.total_match_count, count,
            "independent item/needle membership"
        );
        let scores = scored.results.iter().cloned().collect::<HashMap<_, _>>();
        let mut score_top = scored.results.clone();
        score_top.truncate(limit);
        let cutoff = score_top.last().map_or(0.0, |r| r.1);
        let required_above_cutoff = scores
            .iter()
            .filter(|(_, s)| **s > cutoff)
            .map(|(p, _)| p.clone())
            .collect();
        let all_by_path = all.iter().map(|r| (r.path.clone(), r.clone())).collect();
        let visible = visible_records
            .into_iter()
            .map(|r| (r.path.clone(), r))
            .collect::<HashMap<_, _>>();
        let mut globally_sorted = scored.results;
        semantic_sort(&mut globally_sorted, mode, &visible);
        globally_sorted.truncate(limit);
        let link_paths = if fixture.shape == Shape::InternalLinks {
            fixture
                .records
                .last()
                .map(|r| r.path.clone())
                .into_iter()
                .collect()
        } else {
            HashSet::new()
        };
        Self {
            source,
            all,
            all_by_path,
            visible,
            scores,
            score_top,
            globally_sorted,
            required_above_cutoff,
            link_paths,
            cutoff,
            count,
            limit,
            mode,
            scope,
            empty: query.is_empty(),
            identity: format!(
                "{}:{source:?}:{filter:?}:{query}:{mode:?}:{scope:?}:{limit}",
                fixture.signature()
            ),
        }
    }
    pub(super) fn valid_snapshot(&self, all: &[Entry], visible: &[Entry]) -> bool {
        if !self.valid_entries(all, &self.all_by_path)
            || !self.valid_entries(visible, &self.visible)
        {
            return false;
        }
        if self.source == Source::FileList {
            if !all
                .iter()
                .zip(&self.all)
                .all(|(entry, record)| entry.path == record.path)
            {
                return false;
            }
            let expected_visible = self
                .all
                .iter()
                .filter(|r| self.visible.contains_key(&r.path));
            if !visible
                .iter()
                .zip(expected_visible)
                .all(|(entry, record)| entry.path == record.path)
            {
                return false;
            }
        }
        true
    }
    fn valid_entries(&self, entries: &[Entry], expected: &HashMap<PathBuf, Record>) -> bool {
        if entries.len() != expected.len() {
            return false;
        }
        let mut unique = HashSet::with_capacity(entries.len());
        entries.iter().all(|entry| {
            unique.insert(&entry.path)
                && expected
                    .get(&entry.path)
                    .is_some_and(|record| match entry.kind {
                        Some(kind) => {
                            kind.is_dir == Some(record.is_dir)
                                || (self.link_paths.contains(&entry.path)
                                    && kind.is_link()
                                    && kind.is_dir.is_none())
                        }
                        None => self.source == Source::FileList,
                    })
        })
    }
    pub(super) fn valid_results(&self, rows: &[(PathBuf, f64)], count: usize) -> bool {
        if count != self.count || rows.len() != count.min(self.limit) {
            return false;
        }
        if self.mode != ResultSortMode::Score && self.scope == ResultSortScope::AllMatches {
            return rows == self.globally_sorted;
        }
        if !self.valid_score_subset(rows, false) {
            return false;
        }
        if self.mode == ResultSortMode::Score {
            return self.valid_score_subset(rows, true);
        }
        let mut independently_sorted = rows.to_vec();
        semantic_sort(&mut independently_sorted, self.mode, &self.visible);
        rows == independently_sorted
    }
    pub(super) fn valid_shown_results(
        &self,
        rows: &[(PathBuf, f64)],
        count: usize,
        base: &[(PathBuf, f64)],
    ) -> bool {
        if self.scope != ResultSortScope::ShownResults
            || count != self.count
            || !self.valid_score_subset(base, true)
        {
            return false;
        }
        let mut independently_sorted = base.to_vec();
        semantic_sort(&mut independently_sorted, self.mode, &self.visible);
        rows == independently_sorted
    }
    fn valid_score_subset(&self, rows: &[(PathBuf, f64)], check_order: bool) -> bool {
        if rows.len() != self.count.min(self.limit) {
            return false;
        }
        let unique = rows.iter().map(|r| &r.0).collect::<HashSet<_>>();
        if unique.len() != rows.len()
            || !self
                .required_above_cutoff
                .iter()
                .all(|p| unique.contains(p))
        {
            return false;
        }
        if !rows
            .iter()
            .all(|(p, s)| self.scores.get(p) == Some(s) && *s >= self.cutoff)
        {
            return false;
        }
        if self.source == Source::FileList {
            if check_order {
                return rows == self.score_top;
            }
            let expected = self.score_top.iter().map(|r| &r.0).collect::<HashSet<_>>();
            return unique == expected;
        }
        !check_order || self.empty || rows.windows(2).all(|pair| pair[0].1 >= pair[1].1)
    }
    pub(super) fn expected_count(&self) -> usize {
        self.count
    }
    pub(super) fn signature(&self) -> String {
        let mut hash = 0xcbf29ce484222325u64;
        for byte in self.identity.bytes() {
            hash = (hash ^ u64::from(byte)).wrapping_mul(0x100000001b3);
        }
        format!("{hash:016x}")
    }
}

fn semantic_sort(
    rows: &mut [(PathBuf, f64)],
    mode: ResultSortMode,
    known: &HashMap<PathBuf, Record>,
) {
    if mode == ResultSortMode::Score {
        return;
    }
    rows.sort_by(|a, b| {
        let name_a =
            a.0.file_name()
                .and_then(|n| n.to_str())
                .unwrap_or_default()
                .to_ascii_lowercase();
        let name_b =
            b.0.file_name()
                .and_then(|n| n.to_str())
                .unwrap_or_default()
                .to_ascii_lowercase();
        let path_a = independent_path_key(&a.0);
        let path_b = independent_path_key(&b.0);
        let name_order = name_a.cmp(&name_b).then_with(|| path_a.cmp(&path_b));
        let a_record = &known[&a.0];
        let b_record = &known[&b.0];
        match mode {
            ResultSortMode::NameAsc => name_order,
            ResultSortMode::NameDesc => name_order.reverse(),
            ResultSortMode::PathAsc => path_a.cmp(&path_b),
            ResultSortMode::PathDesc => path_b.cmp(&path_a),
            ResultSortMode::ModifiedAsc => {
                a_record.modified.cmp(&b_record.modified).then(name_order)
            }
            ResultSortMode::ModifiedDesc => {
                b_record.modified.cmp(&a_record.modified).then(name_order)
            }
            ResultSortMode::SizeAsc => {
                independent_size_order(a_record, b_record, false).then(name_order)
            }
            ResultSortMode::SizeDesc => {
                independent_size_order(a_record, b_record, true).then(name_order)
            }
            ResultSortMode::Score => Ordering::Equal,
            ResultSortMode::CreatedAsc | ResultSortMode::CreatedDesc => {
                unreachable!("no fixed creation timestamp")
            }
        }
    });
}
fn independent_size_order(a: &Record, b: &Record, descending: bool) -> Ordering {
    match (a.is_dir, b.is_dir) {
        (false, false) => {
            if descending {
                b.size_bytes.cmp(&a.size_bytes)
            } else {
                a.size_bytes.cmp(&b.size_bytes)
            }
        }
        (false, true) => Ordering::Less,
        (true, false) => Ordering::Greater,
        (true, true) => Ordering::Equal,
    }
}
fn independent_path_key(path: &Path) -> String {
    let key = path.to_string_lossy().replace('\\', "/");
    #[cfg(windows)]
    let key = key.to_ascii_lowercase();
    key
}

fn filter() -> Filter {
    Filter {
        files: true,
        dirs: true,
        ignore_enabled: false,
        ignore_case: false,
    }
}

#[test]
fn tc_229_extended_oracle_rejects_wrong_kind_and_old_nested_member() {
    let f = ExtendedFixture::new(32, Shape::FlatMixed);
    let o = ExtendedOracle::new(
        &f,
        Source::Walker,
        filter(),
        "",
        ResultSortMode::Score,
        ResultSortScope::ShownResults,
        1000,
    );
    let entries = f
        .expected
        .iter()
        .map(|r| {
            if r.is_dir {
                Entry::dir(r.path.clone())
            } else {
                Entry::file(r.path.clone())
            }
        })
        .collect::<Vec<_>>();
    assert!(o.valid_snapshot(&entries, &entries));
    let mut wrong = entries.clone();
    wrong[0] = if f.expected[0].is_dir {
        Entry::file(wrong[0].path.clone())
    } else {
        Entry::dir(wrong[0].path.clone())
    };
    assert!(!o.valid_snapshot(&wrong, &wrong));
    let nested = ExtendedFixture::new(4096, Shape::NestedEarly);
    let oracle = ExtendedOracle::new(
        &nested,
        Source::FileList,
        filter(),
        "",
        ResultSortMode::Score,
        ResultSortScope::ShownResults,
        1000,
    );
    let expected = nested
        .expected
        .iter()
        .map(|r| Entry::unknown(r.path.clone()))
        .collect::<Vec<_>>();
    assert!(oracle.valid_snapshot(&expected, &expected));
    let old = nested
        .records
        .iter()
        .map(|r| Entry::unknown(r.path.clone()))
        .collect::<Vec<_>>();
    assert!(!oracle.valid_snapshot(&old, &old));
}

#[test]
fn tc_229_extended_oracle_ignore_case_changes_membership() {
    let f = ExtendedFixture::new(301, Shape::FlatFiles);
    let mut settings = filter();
    settings.ignore_enabled = true;
    let sensitive = ExtendedOracle::new(
        &f,
        Source::FileList,
        settings,
        "item",
        ResultSortMode::Score,
        ResultSortScope::ShownResults,
        1000,
    );
    settings.ignore_case = true;
    let insensitive = ExtendedOracle::new(
        &f,
        Source::FileList,
        settings,
        "item",
        ResultSortMode::Score,
        ResultSortScope::ShownResults,
        1000,
    );
    assert!(sensitive.expected_count() > insensitive.expected_count());
    let all = f
        .expected
        .iter()
        .map(|r| Entry::unknown(r.path.clone()))
        .collect::<Vec<_>>();
    let case_sensitive_rows = f
        .expected
        .iter()
        .filter(|r| !r.path.to_string_lossy().contains("SKIP"))
        .map(|r| Entry::unknown(r.path.clone()))
        .collect::<Vec<_>>();
    assert!(sensitive.valid_snapshot(&all, &case_sensitive_rows));
    assert!(!insensitive.valid_snapshot(&all, &case_sensitive_rows));
}

#[test]
fn tc_229_extended_oracle_metadata_order_cannot_be_swapped() {
    let f = ExtendedFixture::new(32, Shape::FlatFiles);
    let oracle = ExtendedOracle::new(
        &f,
        Source::Walker,
        filter(),
        "",
        ResultSortMode::ModifiedDesc,
        ResultSortScope::AllMatches,
        32,
    );
    let mut rows = f
        .expected
        .iter()
        .map(|r| (r.path.clone(), 0.0))
        .collect::<Vec<_>>();
    rows.sort_by_key(|(p, _)| {
        std::cmp::Reverse(f.expected.iter().find(|r| &r.path == p).unwrap().modified)
    });
    assert!(oracle.valid_results(&rows, 32));
    rows.swap(0, 1);
    assert!(!oracle.valid_results(&rows, 32));
}

#[test]
fn tc_229_extended_oracle_missing_higher_score_cannot_hide_in_cutoff_ties() {
    let f = ExtendedFixture::new(3, Shape::FlatFiles);
    let mut oracle = ExtendedOracle::new(
        &f,
        Source::Walker,
        filter(),
        "",
        ResultSortMode::Score,
        ResultSortScope::ShownResults,
        2,
    );
    let ranked = f
        .expected
        .iter()
        .zip([100.0, 90.0, 90.0])
        .map(|(r, score)| (r.path.clone(), score))
        .collect::<Vec<_>>();
    oracle.scores = ranked.iter().cloned().collect();
    oracle.score_top = ranked[..2].to_vec();
    oracle.cutoff = 90.0;
    oracle.required_above_cutoff = HashSet::from([ranked[0].0.clone()]);
    assert!(!oracle.valid_results(&ranked[1..], 3));
    assert!(oracle.valid_results(&ranked[..2], 3));
    assert!(oracle.valid_results(&[ranked[0].clone(), ranked[2].clone()], 3));
}

#[test]
fn tc_229_extended_oracle_shown_sort_uses_valid_actual_subset() {
    let f = ExtendedFixture::new(4, Shape::FlatFiles);
    let shown = ExtendedOracle::new(
        &f,
        Source::Walker,
        filter(),
        "",
        ResultSortMode::NameAsc,
        ResultSortScope::ShownResults,
        2,
    );
    // Empty-query Walker ties can choose these rows; Shown sorts this subset,
    // while AllMatches chooses the first two globally name-sorted candidates.
    let base = f.expected[2..]
        .iter()
        .map(|r| (r.path.clone(), 0.0))
        .collect::<Vec<_>>();
    assert!(shown.valid_shown_results(&base, 4, &base));
    let reversed = [base[1].clone(), base[0].clone()];
    assert!(!shown.valid_shown_results(&reversed, 4, &base));
    let all = ExtendedOracle::new(
        &f,
        Source::Walker,
        filter(),
        "",
        ResultSortMode::NameAsc,
        ResultSortScope::AllMatches,
        2,
    );
    assert!(!all.valid_results(&base, 4));
}

#[test]
fn tc_229_extended_oracle_same_root_old_generation_cannot_pass() {
    let original = ExtendedFixture::new(32, Shape::FlatFiles);
    let old = original
        .expected
        .iter()
        .map(|r| Entry::unknown(r.path.clone()))
        .collect::<Vec<_>>();
    let change = original.next_generation();
    let current = change.fixture();
    let oracle = ExtendedOracle::new(
        current,
        Source::FileList,
        filter(),
        "",
        ResultSortMode::Score,
        ResultSortScope::ShownResults,
        1000,
    );
    let latest = current
        .expected
        .iter()
        .map(|r| Entry::unknown(r.path.clone()))
        .collect::<Vec<_>>();
    assert!(oracle.valid_snapshot(&latest, &latest));
    assert!(!oracle.valid_snapshot(&old, &old));
}

#[test]
fn tc_229_extended_oracle_size_sort_keeps_directory_missing_values_last() {
    let fixture = ExtendedFixture::new(32, Shape::FlatMixed);
    let oracle = ExtendedOracle::new(
        &fixture,
        Source::Walker,
        filter(),
        "",
        ResultSortMode::SizeAsc,
        ResultSortScope::AllMatches,
        32,
    );
    let mut records = fixture.expected.iter().collect::<Vec<_>>();
    records.sort_by(|a, b| {
        a.is_dir.cmp(&b.is_dir).then_with(|| {
            if a.is_dir {
                a.path.cmp(&b.path)
            } else {
                a.size_bytes
                    .cmp(&b.size_bytes)
                    .then_with(|| a.path.cmp(&b.path))
            }
        })
    });
    let rows = records
        .iter()
        .map(|r| (r.path.clone(), 0.0))
        .collect::<Vec<_>>();
    assert!(oracle.valid_results(&rows, 32));
    let mut wrong = rows.clone();
    let first_directory = records.iter().position(|r| r.is_dir).unwrap();
    wrong.swap(0, first_directory);
    assert!(!oracle.valid_results(&wrong, 32));
}

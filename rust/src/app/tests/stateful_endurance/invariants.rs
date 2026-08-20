use std::collections::HashSet;

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct SemanticSnapshot {
    pub(super) tab_ids: Vec<u64>,
    pub(super) active_tab: usize,
    pub(super) active_root: usize,
    pub(super) active_query: String,
    pub(super) results_len: usize,
    pub(super) total_match_count: usize,
    pub(super) current_row: Option<usize>,
    pub(super) index_pending: usize,
    pub(super) index_inflight: usize,
    pub(super) routed_tab_ids: Vec<u64>,
    pub(super) active_index_pending: bool,
    pub(super) active_search_pending: bool,
}

impl SemanticSnapshot {
    pub(super) fn digest(&self) -> String {
        format!(
            "tabs={:?};active={};root={};query={:?};results={}/{};row={:?};index={}/{};routes={:?};pending={}/{}",
            self.tab_ids,
            self.active_tab,
            self.active_root,
            self.active_query,
            self.results_len,
            self.total_match_count,
            self.current_row,
            self.index_pending,
            self.index_inflight,
            self.routed_tab_ids,
            self.active_index_pending,
            self.active_search_pending,
        )
    }
}

pub(super) fn validate(snapshot: &SemanticSnapshot) -> Result<(), String> {
    if snapshot.tab_ids.is_empty() {
        return Err("tab list must not be empty".to_string());
    }
    if snapshot.active_tab >= snapshot.tab_ids.len() {
        return Err(format!(
            "active tab {} is outside {} tabs",
            snapshot.active_tab,
            snapshot.tab_ids.len()
        ));
    }
    let unique = snapshot.tab_ids.iter().copied().collect::<HashSet<_>>();
    if unique.len() != snapshot.tab_ids.len() {
        return Err("tab ids must be unique".to_string());
    }
    if snapshot.results_len > snapshot.total_match_count {
        return Err(format!(
            "results {} exceed total {}",
            snapshot.results_len, snapshot.total_match_count
        ));
    }
    if let Some(row) = snapshot.current_row {
        let valid = row < snapshot.results_len || (snapshot.results_len == 0 && row == 0);
        if !valid {
            return Err(format!(
                "current row {row} is outside {} results",
                snapshot.results_len
            ));
        }
    }
    if snapshot.index_pending > 4 {
        return Err(format!(
            "index pending {} exceeds coordinator bound 4",
            snapshot.index_pending
        ));
    }
    if snapshot.index_inflight > 2 {
        return Err(format!(
            "index inflight {} exceeds coordinator bound 2",
            snapshot.index_inflight
        ));
    }
    if let Some(orphan) = snapshot
        .routed_tab_ids
        .iter()
        .find(|tab_id| !unique.contains(tab_id))
    {
        return Err(format!("request route references closed tab {orphan}"));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn valid_snapshot() -> SemanticSnapshot {
        SemanticSnapshot {
            tab_ids: vec![1],
            active_tab: 0,
            active_root: 0,
            active_query: String::new(),
            results_len: 1,
            total_match_count: 1,
            current_row: Some(0),
            index_pending: 0,
            index_inflight: 0,
            routed_tab_ids: Vec::new(),
            active_index_pending: false,
            active_search_pending: false,
        }
    }

    #[test]
    fn tc_181_invariant_checker_rejects_corrupt_snapshots() {
        let mut corrupt = valid_snapshot();
        corrupt.tab_ids.clear();
        assert!(validate(&corrupt)
            .unwrap_err()
            .contains("must not be empty"));

        let mut corrupt = valid_snapshot();
        corrupt.tab_ids = vec![1, 1];
        assert!(validate(&corrupt).unwrap_err().contains("unique"));

        let mut corrupt = valid_snapshot();
        corrupt.active_tab = 1;
        assert!(validate(&corrupt).unwrap_err().contains("outside"));

        let mut corrupt = valid_snapshot();
        corrupt.results_len = 2;
        assert!(validate(&corrupt).unwrap_err().contains("exceed"));

        let mut corrupt = valid_snapshot();
        corrupt.current_row = Some(2);
        assert!(validate(&corrupt).unwrap_err().contains("current row"));

        let mut corrupt = valid_snapshot();
        corrupt.index_pending = 5;
        assert!(validate(&corrupt).unwrap_err().contains("bound 4"));

        let mut corrupt = valid_snapshot();
        corrupt.index_inflight = 3;
        assert!(validate(&corrupt).unwrap_err().contains("bound 2"));

        let mut corrupt = valid_snapshot();
        corrupt.routed_tab_ids.push(99);
        assert!(validate(&corrupt).unwrap_err().contains("closed tab"));
    }
}

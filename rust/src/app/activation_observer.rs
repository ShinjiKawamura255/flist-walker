//! Opt-in, bounded test instrumentation. Absent from product builds.
//! The ordinary numeric policy remains unset; observation is not a UX guarantee.

use serde_json::Value;

pub(super) fn admit(record: &Value) -> &'static str {
    let keys = ["first_model", "first_frame", "work", "cleanup"];
    if keys
        .iter()
        .any(|key| record["guards"][key].as_str() == Some("FAIL"))
    {
        return "FAIL";
    }
    if keys
        .iter()
        .any(|key| record["guards"][key].as_str() != Some("PASS"))
        || record["clock_valid"].as_bool() != Some(true)
        || record["normal_exit"].as_bool() != Some(true)
        || record["timed_out"].as_bool() != Some(false)
    {
        return "INDETERMINATE";
    }
    "OBSERVATION_VALID"
}

use super::FlistWalkerApp;
use serde::Serialize;
use std::collections::{BTreeMap, HashSet};
use std::path::{Path, PathBuf};
use std::time::Instant;

const MAX_INTENTS: usize = 8;
const MAX_ROWS: usize = 256;

#[derive(Clone, Debug, Serialize)]
pub(super) struct Receipt {
    pub(super) at_ns: Option<u64>,
    pub(super) tab: Option<u64>,
    pub(super) root: PathBuf,
    pub(super) generation: Option<u64>,
    pub(super) source: String,
    pub(super) query: String,
    pub(super) sort: String,
    pub(super) sort_scope: String,
    pub(super) results: Vec<(PathBuf, f64)>,
    pub(super) current_row: Option<usize>,
    pub(super) evicted_selected_path: Option<PathBuf>,
    pub(super) pinned: Vec<PathBuf>,
    pub(super) drawn: Vec<(usize, PathBuf)>,
}

#[derive(Clone, Debug, Serialize)]
pub(super) struct Intent {
    pub(super) id: usize,
    pub(super) kind: &'static str,
    pub(super) target: Option<u64>,
    pub(super) root: PathBuf,
    pub(super) lifecycle: String,
    pub(super) committed: bool,
    pub(super) warm: bool,
    pub(super) t0_ns: Option<u64>,
    pub(super) ingress_metadata_done_ns: Option<u64>,
    pub(super) request_floor: u64,
    pub(super) attempts: usize,
    pub(super) first_retry_ns: Option<u64>,
    pub(super) last_attempt_ns: Option<u64>,
    pub(super) canceled: bool,
    pub(super) first_model: Option<Receipt>,
    pub(super) first_frame: Option<Receipt>,
    pub(super) first_later_failure: Option<Receipt>,
    pub(super) first_later_unknown: Option<Receipt>,
}

#[derive(Clone, Debug)]
pub(super) struct Probe {
    pub(super) origin: Instant,
    pub(super) intents: Vec<Intent>,
    pub(super) generations: BTreeMap<u64, u64>,
    pub(super) sources: BTreeMap<u64, String>,
    pub(super) oracle: BTreeMap<PathBuf, Vec<PathBuf>>,
    frame: Option<Receipt>,
    pub(super) clock_valid: bool,
}

impl Probe {
    pub(super) fn new(
        origin: Instant,
        root: PathBuf,
        oracle: BTreeMap<PathBuf, Vec<PathBuf>>,
    ) -> Self {
        Self {
            origin,
            intents: vec![Intent {
                id: 1,
                kind: "bootstrap",
                target: None,
                root,
                lifecycle: "Uninitialized".into(),
                committed: false,
                warm: false,
                t0_ns: Some(0),
                ingress_metadata_done_ns: None,
                request_floor: 1,
                attempts: 1,
                first_retry_ns: None,
                last_attempt_ns: Some(0),
                canceled: false,
                first_model: None,
                first_frame: None,
                first_later_failure: None,
                first_later_unknown: None,
            }],
            generations: BTreeMap::new(),
            sources: BTreeMap::new(),
            oracle,
            frame: None,
            clock_valid: true,
        }
    }

    pub(super) fn ns(&self, at: Instant) -> Option<u64> {
        at.checked_duration_since(self.origin)
            .and_then(|d| u64::try_from(d.as_nanos()).ok())
    }

    pub(super) fn cancel_pending(&mut self, target: Option<u64>) {
        if let Some(intent) = self.intents.last_mut() {
            if target.is_some() && intent.target == target && intent.first_frame.is_none() {
                intent.canceled = true;
            }
        }
    }

    fn check(&self, receipt: &Receipt, root: &Path, frame: bool) -> &'static str {
        let Some(expected) = self.oracle.get(root) else {
            return "UNKNOWN";
        };
        if receipt.root != root
            || !receipt.query.is_empty()
            || receipt.sort != "Score"
            || receipt.sort_scope != "ShownResults"
            || !receipt.pinned.is_empty()
            || receipt.evicted_selected_path.is_some()
            || receipt.current_row != Some(0)
            || receipt.results.is_empty()
            || receipt.results.len() > expected.len()
        {
            return "FAIL";
        }
        let mut seen = HashSet::new();
        for (path, score) in &receipt.results {
            if !expected.contains(path) || !seen.insert(path) || !score.is_finite() || *score != 0.0
            {
                return "FAIL";
            }
        }
        if receipt.source.starts_with("FileList")
            && receipt
                .results
                .iter()
                .map(|r| &r.0)
                .ne(expected.iter().take(receipt.results.len()))
        {
            return "FAIL";
        }
        if frame
            && (receipt.drawn.is_empty()
                || receipt
                    .drawn
                    .iter()
                    .any(|(i, path)| receipt.results.get(*i).map(|r| &r.0) != Some(path)))
        {
            return "FAIL";
        }
        if receipt.generation.is_none()
            || receipt.at_ns.is_none()
            || !(receipt.source.starts_with("Walker") || receipt.source.starts_with("FileList"))
        {
            return "UNKNOWN";
        }
        if receipt
            .tab
            .and_then(|t| self.generations.get(&t).copied())
            .is_some_and(|g| Some(g) != receipt.generation)
        {
            return "FAIL";
        }
        if receipt
            .tab
            .and_then(|t| self.sources.get(&t))
            .is_some_and(|s| s != &receipt.source)
        {
            return "FAIL";
        }
        "PASS"
    }

    pub(super) fn receipt_guard(
        &self,
        receipt: Option<&Receipt>,
        intent: &Intent,
        frame: bool,
    ) -> &'static str {
        let Some(receipt) = receipt else {
            return "UNKNOWN";
        };
        if receipt.tab.is_none() {
            return "UNKNOWN";
        }
        if receipt.tab != intent.target {
            return "FAIL";
        }
        self.check(receipt, &intent.root, frame)
    }
}

impl FlistWalkerApp {
    pub(super) fn activation_receipt(&self, probe: &Probe) -> Receipt {
        let r = &self.shell.runtime;
        let tab = self.current_tab_id();
        assert!(
            r.results.len() <= MAX_ROWS,
            "activation observer result bound"
        );
        Receipt {
            at_ns: probe.ns(Instant::now()),
            tab,
            root: r.root.clone(),
            generation: tab.and_then(|t| probe.generations.get(&t).copied()),
            source: tab
                .and_then(|t| probe.sources.get(&t).cloned())
                .unwrap_or_else(|| "Unknown".into()),
            query: r.query_state.query.clone(),
            sort: format!("{:?}", r.result_sort_mode),
            sort_scope: format!("{:?}", r.result_sort_scope),
            results: r.results.clone(),
            current_row: r.current_row,
            evicted_selected_path: r.evicted_selected_path.clone(),
            pinned: r.pinned_paths.iter().cloned().collect(),
            drawn: Vec::new(),
        }
    }

    pub(super) fn observe_activation_ingress(&mut self, next: usize, entered: Instant) {
        if self.activation_observer.is_none() {
            return;
        }
        let Some(tab) = self.shell.tabs.get(next) else {
            return;
        };
        let id = tab.id;
        let root = tab.root.clone();
        let lifecycle = format!("{:?}", tab.index_state.lifecycle());
        let committed = tab.index_state.committed_snapshot_present();
        let same_active = next == self.shell.tabs.active_tab_index();
        let pending = self.shell.tabs.pending_activation_tab_id;
        let warm = self.shell.indexing.warm_tab_id == Some(id);
        let metadata_done = Instant::now();
        let Some(probe) = self.activation_observer.as_mut() else {
            return;
        };
        let at = probe.ns(entered);
        let metadata_done_ns = probe.ns(metadata_done);
        probe.clock_valid &= at
            .zip(metadata_done_ns)
            .is_some_and(|(start, done)| start <= done);
        if same_active {
            probe.cancel_pending(pending);
            return;
        }
        if pending == Some(id)
            && probe
                .intents
                .last()
                .is_some_and(|i| i.target == Some(id) && !i.canceled)
        {
            let intent = probe.intents.last_mut().unwrap();
            intent.attempts += 1;
            if intent.first_retry_ns.is_none() {
                intent.first_retry_ns = at;
            }
            intent.last_attempt_ns = at;
            return;
        }
        probe.cancel_pending(pending);
        assert!(
            probe.intents.len() < MAX_INTENTS,
            "activation intent observer overflow"
        );
        let intent_id = probe.intents.len() + 1;
        probe.intents.push(Intent {
            id: intent_id,
            kind: "tab-switch",
            target: Some(id),
            root,
            lifecycle,
            committed,
            warm,
            t0_ns: at,
            ingress_metadata_done_ns: metadata_done_ns,
            request_floor: self.shell.indexing.next_request_id,
            attempts: 1,
            first_retry_ns: None,
            last_attempt_ns: at,
            canceled: false,
            first_model: None,
            first_frame: None,
            first_later_failure: None,
            first_later_unknown: None,
        });
        probe.frame = None;
    }

    pub(super) fn observe_activation_model(&mut self) {
        let Some(probe) = &self.activation_observer else {
            return;
        };
        let Some(intent) = probe.intents.last() else {
            return;
        };
        if probe.oracle.is_empty() {
            return;
        }
        if intent.canceled
            || intent.target != self.current_tab_id()
            || (self.shell.runtime.results.is_empty() && intent.first_model.is_none())
        {
            return;
        }
        let receipt = self.activation_receipt(probe);
        let guard = probe.receipt_guard(Some(&receipt), intent, false);
        let probe = self.activation_observer.as_mut().unwrap();
        let intent = probe.intents.last_mut().unwrap();
        if intent.first_model.is_none() {
            intent.first_model = Some(receipt);
        } else if guard == "FAIL" && intent.first_later_failure.is_none() {
            intent.first_later_failure = Some(receipt);
        } else if guard == "UNKNOWN" && intent.first_later_unknown.is_none() {
            intent.first_later_unknown = Some(receipt);
        }
    }

    pub(super) fn observe_activation_drawn_row(&mut self, row: usize, path: &Path) {
        let Some(probe) = &self.activation_observer else {
            return;
        };
        let Some(intent) = probe.intents.last() else {
            return;
        };
        if probe.oracle.is_empty() {
            return;
        }
        if intent.canceled || intent.target != self.current_tab_id() {
            return;
        }
        let receipt = probe
            .frame
            .is_none()
            .then(|| self.activation_receipt(probe));
        let probe = self.activation_observer.as_mut().unwrap();
        if let Some(receipt) = receipt {
            probe.frame = Some(receipt);
        }
        let frame = probe.frame.as_mut().unwrap();
        assert!(
            frame.drawn.len() < MAX_ROWS,
            "activation frame observer overflow"
        );
        frame.drawn.push((row, path.to_path_buf()));
    }

    pub(super) fn finish_activation_headless_frame(&mut self) {
        let Some(probe) = self.activation_observer.as_mut() else {
            return;
        };
        let Some(mut frame) = probe.frame.take() else {
            return;
        };
        frame.at_ns = probe.ns(Instant::now());
        let intent = probe.intents.last().unwrap();
        let guard = probe.receipt_guard(Some(&frame), intent, true);
        let intent = probe.intents.last_mut().unwrap();
        if intent.first_frame.is_none() {
            intent.first_frame = Some(frame);
        } else if guard == "FAIL" && intent.first_later_failure.is_none() {
            intent.first_later_failure = Some(frame);
        } else if guard == "UNKNOWN" && intent.first_later_unknown.is_none() {
            intent.first_later_unknown = Some(frame);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn healthy() -> Value {
        json!({"guards":{"first_model":"PASS","first_frame":"PASS","work":"PASS","cleanup":"PASS"},
            "clock_valid":true,"normal_exit":true,"timed_out":false})
    }

    #[test]
    fn tc_234_b0_admission_rejects_missing_and_failed_immutable_first_evidence() {
        assert_eq!(admit(&healthy()), "OBSERVATION_VALID");
        for key in ["first_model", "first_frame", "work", "cleanup"] {
            let mut missing = healthy();
            missing["guards"].as_object_mut().unwrap().remove(key);
            assert_eq!(admit(&missing), "INDETERMINATE", "{key}");
            let mut failed = healthy();
            failed["guards"][key] = json!("FAIL");
            failed["later_receipt"] = json!("PASS");
            failed["clock_valid"] = json!(false);
            assert_eq!(admit(&failed), "FAIL", "correctness dominates {key}");
        }
        for key in ["clock_valid", "normal_exit"] {
            let mut bad = healthy();
            bad[key] = json!(false);
            assert_eq!(admit(&bad), "INDETERMINATE");
        }
        let mut timeout = healthy();
        timeout["timed_out"] = json!(true);
        assert_eq!(admit(&timeout), "INDETERMINATE");
    }
}

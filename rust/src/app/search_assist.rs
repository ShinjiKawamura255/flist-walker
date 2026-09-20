//! GUI-only assistance. Query interpretation remains owned by the shared parser.
use super::{FlistWalkerApp, TabResourceLifecycle};
use crate::indexer::MaxDepth;
use eframe::egui;
use std::ops::Range;
use std::path::PathBuf;
use std::time::{Duration, Instant};

const IDLE_DELAY: Duration = Duration::from_millis(300);
const MAX_ADVICE_BYTES: usize = 4096;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum QueryAdvice {
    Pending(&'static str),
    Typo {
        range: Range<usize>,
        replacement: &'static str,
    },
}

fn query_advice(query: &str) -> Option<QueryAdvice> {
    if query.len() > MAX_ADVICE_BYTES {
        return None;
    }
    let mut offset = 0;
    for token in query.split_whitespace() {
        let start = offset + query[offset..].find(token)?;
        offset = start + token.len();
        let (term, prefix_start) = token
            .strip_prefix('!')
            .map_or((token, start), |s| (s, start + 1));
        let Some((prefix, value)) = term.split_once(':') else {
            continue;
        };
        const FIELDS: [&str; 4] = ["name", "path", "dir", "ext"];
        if let Some(field) = FIELDS.into_iter().find(|field| *field == prefix) {
            if value.is_empty() {
                return Some(QueryAdvice::Pending(field));
            }
            continue;
        }
        // These are field-like tokens only, not drive prefixes, URLs, literals or regex groups.
        if !(2..=5).contains(&prefix.len())
            || !prefix.bytes().all(|b| b.is_ascii_alphabetic())
            || value.starts_with('/')
            || value.starts_with('\\')
        {
            continue;
        }
        let lower = prefix.to_ascii_lowercase();
        let mut candidates = FIELDS
            .into_iter()
            .filter(|field| one_edit_apart(&lower, field));
        if let (Some(replacement), None) = (candidates.next(), candidates.next()) {
            return Some(QueryAdvice::Typo {
                range: prefix_start..prefix_start + prefix.len(),
                replacement,
            });
        }
    }
    None
}

// Small ASCII field names only; cost is bounded independently of candidate count.
fn one_edit_apart(a: &str, b: &str) -> bool {
    let (a, b) = (a.as_bytes(), b.as_bytes());
    if a.len().abs_diff(b.len()) > 1 {
        return false;
    }
    if a.len() == b.len() {
        let different: Vec<_> = (0..a.len()).filter(|&i| a[i] != b[i]).collect();
        return different.len() <= 1
            || (different.len() == 2
                && different[1] == different[0] + 1
                && a[different[0]] == b[different[1]]
                && a[different[1]] == b[different[0]]);
    }
    let (short, long) = if a.len() < b.len() { (a, b) } else { (b, a) };
    let skip = short
        .iter()
        .zip(long)
        .position(|(a, b)| a != b)
        .unwrap_or(short.len());
    short[skip..] == long[skip + 1..]
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum FilterValue {
    Kind(bool, bool),
    Depth(MaxDepth),
    Ignore(bool),
    Case(bool),
    Regex(bool),
}

impl FilterValue {
    fn same_setting(self, other: Self) -> bool {
        std::mem::discriminant(&self) == std::mem::discriminant(&other)
    }
    fn description(self) -> &'static str {
        match self {
            Self::Kind(..) => "File/folder filter relaxed",
            Self::Depth(_) => "Depth limit removed",
            Self::Ignore(_) => "Ignore List disabled",
            Self::Case(_) => "Case sensitivity disabled",
            Self::Regex(_) => "Regex disabled",
        }
    }
}

#[derive(Debug)]
struct FilterUndo {
    before: FilterValue,
    after: FilterValue,
}

#[derive(Default)]
pub(super) struct SearchAssist {
    context: Option<(Option<u64>, PathBuf)>,
    query: String,
    query_too_long: bool,
    changed_at: Option<Instant>,
    advice: Option<QueryAdvice>,
    undo: Option<FilterUndo>,
}

impl SearchAssist {
    pub(super) fn note_query_edit(&mut self) {
        self.changed_at = Some(Instant::now());
    }
}

#[derive(Debug, PartialEq, Eq)]
enum EmptyState {
    Hidden,
    Waiting,
    Searching,
    Indexing,
    Invalid,
    SearchFailed,
    IndexFailed,
    NoMatches,
}

impl FlistWalkerApp {
    pub(super) fn invalidate_filter_undo(&mut self) {
        self.shell.ui.search_assist.undo = None;
    }

    pub(super) fn manual_filter_changed(&mut self, setting: FilterValue) {
        if self
            .shell
            .ui
            .search_assist
            .undo
            .as_ref()
            .is_some_and(|u| u.after.same_setting(setting))
        {
            self.invalidate_filter_undo();
        }
    }

    fn filter_value(&self, setting: FilterValue) -> FilterValue {
        match setting {
            FilterValue::Kind(..) => FilterValue::Kind(
                self.shell.runtime.include_files,
                self.shell.runtime.include_dirs,
            ),
            FilterValue::Depth(_) => FilterValue::Depth(self.shell.runtime.max_depth),
            FilterValue::Ignore(_) => FilterValue::Ignore(self.shell.ui.ignore_list_enabled),
            FilterValue::Case(_) => FilterValue::Case(self.shell.runtime.ignore_case),
            FilterValue::Regex(_) => FilterValue::Regex(self.shell.runtime.use_regex),
        }
    }

    fn sync_search_assist(&mut self, now: Instant) {
        let context = (self.current_tab_id(), self.shell.runtime.root.clone());
        let undo_valid = self
            .shell
            .ui
            .search_assist
            .undo
            .as_ref()
            .is_none_or(|u| self.filter_value(u.after) == u.after);
        let assist = &mut self.shell.ui.search_assist;
        if assist.context.as_ref() != Some(&context) {
            *assist = SearchAssist {
                context: Some(context),
                changed_at: Some(now),
                ..Default::default()
            };
        }
        if !undo_valid {
            assist.undo = None;
        }
        let query = &self.shell.runtime.query_state.query;
        if query.len() > MAX_ADVICE_BYTES {
            assist.query.clear();
            assist.query_too_long = true;
            assist.advice = None;
            // Normal search still accepts large input; input events own its idle timestamp.
            assist.changed_at.get_or_insert(now);
        } else if assist.query != *query || assist.query_too_long {
            assist.query = query.clone();
            assist.query_too_long = false;
            assist.changed_at = Some(now);
            assist.advice = query_advice(query);
        }
        if self.shell.ui.ime_composition_active {
            assist.changed_at = Some(now);
        }
    }

    fn current_search_error(&self) -> Option<&str> {
        self.shell
            .runtime
            .query_state
            .search_error
            .as_ref()
            .filter(|(query, _)| query == &self.shell.runtime.query_state.query)
            .map(|(_, error)| error.as_str())
    }

    fn empty_result_state(&self, now: Instant) -> EmptyState {
        if !self.shell.runtime.results.is_empty() {
            return EmptyState::Hidden;
        }
        if self.shell.search.worker_unavailable() {
            return EmptyState::SearchFailed;
        }
        if self.shell.ui.ime_composition_active {
            return EmptyState::Waiting;
        }
        if matches!(
            self.shell.ui.search_assist.advice,
            Some(QueryAdvice::Pending(_))
        ) {
            return EmptyState::Invalid;
        }
        if self.shell.indexing.in_progress || self.shell.indexing.pending_finish.is_some() {
            return EmptyState::Indexing;
        }
        if self.shell.search.in_progress() || self.shell.worker_bus.sort.in_progress {
            return EmptyState::Searching;
        }
        if self.shell.indexing.lifecycle() == TabResourceLifecycle::Failed {
            return EmptyState::IndexFailed;
        }
        if self.current_search_error().is_some() {
            return EmptyState::SearchFailed;
        }
        if self
            .shell
            .ui
            .search_assist
            .changed_at
            .is_some_and(|t| now.saturating_duration_since(t) < IDLE_DELAY)
        {
            return EmptyState::Waiting;
        }
        EmptyState::NoMatches
    }

    fn repair_query_prefix(&mut self, range: Range<usize>, replacement: &str) {
        if self.shell.ui.ime_composition_active
            || self.shell.ui.search_assist.query != self.shell.runtime.query_state.query
        {
            return;
        }
        self.shell
            .runtime
            .query_state
            .query
            .replace_range(range, replacement);
        self.mark_query_edited();
        self.update_results();
        self.finish_programmatic_query_replacement();
    }

    fn apply_assist_filter(&mut self, value: FilterValue) {
        self.shell.runtime.query_state.search_error = None;
        match value {
            FilterValue::Kind(files, dirs) => {
                let files_changed = self.shell.runtime.include_files != files;
                let dirs_changed = self.shell.runtime.include_dirs != dirs;
                self.shell.runtime.include_files = files;
                self.shell.runtime.include_dirs = dirs;
                self.maybe_reindex_from_filter_toggles(false, files_changed, dirs_changed, false);
            }
            FilterValue::Depth(depth) => {
                self.shell.runtime.max_depth = depth;
                self.shell.tabs.mark_active_tab_meaningfully_engaged();
                self.sync_active_tab_state();
                self.mark_ui_state_dirty();
                self.persist_ui_state_now();
                self.request_index_refresh();
            }
            FilterValue::Ignore(enabled) => {
                self.shell.ui.ignore_list_enabled = enabled;
                self.mark_ui_state_dirty();
                self.persist_ui_state_now();
                self.maybe_reindex_from_filter_toggles(false, false, false, true);
            }
            FilterValue::Case(ignore_case) => {
                self.shell.runtime.ignore_case = ignore_case;
                self.shell.tabs.mark_active_tab_meaningfully_engaged();
                self.invalidate_result_sort(true);
                self.update_results();
            }
            FilterValue::Regex(regex) => {
                self.shell.runtime.use_regex = regex;
                self.shell.tabs.mark_active_tab_meaningfully_engaged();
                self.invalidate_result_sort(true);
                self.update_results();
            }
        }
    }

    fn relax_filter(&mut self, after: FilterValue) {
        self.sync_search_assist(Instant::now());
        let before = self.filter_value(after);
        if before == after {
            return;
        }
        self.apply_assist_filter(after);
        self.shell.ui.search_assist.undo = Some(FilterUndo { before, after });
        self.request_focus_query();
    }

    fn undo_filter_relaxation(&mut self) {
        self.sync_search_assist(Instant::now());
        if let Some(undo) = self.shell.ui.search_assist.undo.take() {
            self.apply_assist_filter(undo.before);
        }
        self.request_focus_query();
    }

    pub(super) fn render_query_assistance(&mut self, ui: &mut egui::Ui) {
        self.sync_search_assist(Instant::now());
        if self.shell.search.worker_unavailable() {
            ui.colored_label(
                ui.visuals().error_fg_color,
                "Search worker is unavailable. Restart FlistWalker to resume searching.",
            );
        }
        if self.shell.runtime.query_state.history_search_active
            || self.shell.ui.ime_composition_active
        {
            return;
        }
        let advice = self.shell.ui.search_assist.advice.clone();
        let input_pending = matches!(advice, Some(QueryAdvice::Pending(_)));
        match advice {
            Some(QueryAdvice::Pending(field)) => {
                ui.weak(format!("Enter a value after {field}:"));
            }
            Some(QueryAdvice::Typo { range, replacement }) => {
                ui.horizontal_wrapped(|ui| {
                    ui.label(format!("Did you mean {replacement}: ?"));
                    if ui.button(format!("Change to {replacement}:")).clicked() {
                        self.repair_query_prefix(range, replacement);
                    }
                });
            }
            None => {}
        }
        // An optional spelling suggestion must not hide another term's error or a
        // worker failure. Empty known fields retain their non-error input prompt.
        if !input_pending
            && !self.shell.search.in_progress()
            && !self.shell.search.worker_unavailable()
        {
            if let Some(error) = self.current_search_error() {
                ui.colored_label(ui.visuals().error_fg_color, error);
            }
        }
        if let Some(undo) = &self.shell.ui.search_assist.undo {
            let description = undo.after.description();
            ui.horizontal_wrapped(|ui| {
                ui.label(description);
                if ui.button("Undo filter change").clicked() {
                    self.undo_filter_relaxation();
                }
            });
        }
    }

    pub(super) fn render_empty_result_assistance(&mut self, ui: &mut egui::Ui) {
        let now = Instant::now();
        self.sync_search_assist(now);
        let state = self.empty_result_state(now);
        match state {
            EmptyState::Hidden => {}
            EmptyState::Waiting => {
                ui.ctx().request_repaint_after(IDLE_DELAY);
            }
            EmptyState::Searching => {
                ui.label("Searching...");
            }
            EmptyState::Indexing => {
                ui.label("Scanning search root...");
            }
            EmptyState::Invalid => {
                ui.label("Complete or correct the query above to search.");
            }
            EmptyState::SearchFailed => {
                ui.label("Search could not be completed. See the message above.");
            }
            EmptyState::IndexFailed => {
                ui.label("Could not load the search root.");
                ui.label(&self.shell.runtime.notice);
                if ui.button("Retry loading").clicked() {
                    self.request_index_refresh();
                }
            }
            EmptyState::NoMatches => {
                ui.label("No matching files or folders.");
                let mut filters = Vec::new();
                if (!self.shell.runtime.include_files || !self.shell.runtime.include_dirs)
                    && !self.use_filelist_requires_locked_filters()
                {
                    let label = if self.shell.runtime.include_files {
                        "Files only — include folders"
                    } else {
                        "Folders only — include files"
                    };
                    filters.push((label.to_string(), FilterValue::Kind(true, true)));
                }
                if let Some(depth) = self.shell.runtime.max_depth.value() {
                    filters.push((
                        format!("Depth ≤ {depth} — remove limit"),
                        FilterValue::Depth(MaxDepth::unlimited()),
                    ));
                }
                if self.shell.ui.ignore_list_enabled
                    && !self.shell.runtime.ignore_list_terms.is_empty()
                {
                    filters.push((
                        "Ignore List — disable".to_string(),
                        FilterValue::Ignore(false),
                    ));
                }
                if !self.shell.runtime.ignore_case {
                    filters.push((
                        "Case sensitive — ignore case".to_string(),
                        FilterValue::Case(true),
                    ));
                }
                if self.shell.runtime.use_regex {
                    filters.push((
                        "Regex — use plain search".to_string(),
                        FilterValue::Regex(false),
                    ));
                }
                if !filters.is_empty() {
                    ui.weak("Active conditions (change one and search again):");
                    ui.horizontal_wrapped(|ui| {
                        for (label, value) in filters {
                            if ui.button(label).clicked() {
                                self.relax_filter(value);
                            }
                        }
                    });
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::tab_state::TabResourceTransition;
    use crate::app::tests::{test_root, test_settings_scope};

    fn settled(app: &mut FlistWalkerApp) {
        app.shell.indexing.in_progress = false;
        app.shell.indexing.pending_finish = None;
        app.shell
            .indexing
            .apply_resource_transition(TabResourceTransition::Success);
        app.shell.search.clear_active_request_state();
        app.shell.worker_bus.sort.clear_request();
        app.shell.runtime.committed_for_test_mut().results.clear();
    }

    #[test]
    fn ux_empty_guidance_waits_for_idle_and_latest_workers() {
        let scope = test_settings_scope("ux-idle");
        let mut app = scope.app(test_root("ux-idle"), 50, "none".into());
        settled(&mut app);
        let now = Instant::now();
        app.sync_search_assist(now);
        assert_eq!(app.empty_result_state(now), EmptyState::Waiting);
        assert_eq!(
            app.empty_result_state(now + IDLE_DELAY),
            EmptyState::NoMatches
        );
        app.shell.search.set_in_progress(true);
        assert_eq!(
            app.empty_result_state(now + IDLE_DELAY),
            EmptyState::Searching
        );
        app.shell.indexing.in_progress = true;
        assert_eq!(
            app.empty_result_state(now + IDLE_DELAY),
            EmptyState::Indexing
        );
        settled(&mut app);
        app.shell
            .indexing
            .apply_resource_transition(TabResourceTransition::Failure);
        assert_eq!(
            app.empty_result_state(now + IDLE_DELAY),
            EmptyState::IndexFailed
        );
        settled(&mut app);
        app.shell.ui.ime_composition_active = true;
        assert_eq!(
            app.empty_result_state(now + IDLE_DELAY),
            EmptyState::Waiting
        );
    }

    #[test]
    fn ux_search_dispatch_failure_is_not_reported_as_no_matches() {
        let scope = test_settings_scope("ux-unavailable");
        let mut app = scope.app(test_root("ux-unavailable"), 50, "report".into());
        settled(&mut app);
        let (tx, rx) = std::sync::mpsc::channel();
        app.shell.search.tx = tx;
        drop(rx);
        app.update_results();
        let now = Instant::now();
        app.sync_search_assist(now);
        assert_eq!(
            app.current_search_error(),
            Some("Search worker is unavailable")
        );
        assert_eq!(
            app.empty_result_state(now + IDLE_DELAY),
            EmptyState::SearchFailed
        );
    }

    #[test]
    fn ux_depth_kind_and_ignore_relaxations_preserve_query_and_restore_only_their_value() {
        let scope = test_settings_scope("ux-filter-types");
        let mut app = scope.app(test_root("ux-filter-types"), 50, "report".into());
        app.shell.runtime.use_filelist = false;
        app.shell.runtime.include_files = true;
        app.shell.runtime.include_dirs = false;
        app.relax_filter(FilterValue::Kind(true, true));
        assert!(app.shell.runtime.include_dirs);
        app.undo_filter_relaxation();
        assert!(!app.shell.runtime.include_dirs);
        app.shell.runtime.max_depth = MaxDepth::limited(2).unwrap();
        app.relax_filter(FilterValue::Depth(MaxDepth::unlimited()));
        assert_eq!(app.shell.runtime.max_depth, MaxDepth::unlimited());
        app.undo_filter_relaxation();
        assert_eq!(app.shell.runtime.max_depth.value(), Some(2));
        app.shell.ui.ignore_list_enabled = true;
        app.relax_filter(FilterValue::Ignore(false));
        assert!(!app.shell.ui.ignore_list_enabled);
        app.undo_filter_relaxation();
        assert!(app.shell.ui.ignore_list_enabled);
        assert_eq!(app.shell.runtime.query_state.query, "report");
    }

    #[test]
    fn ux_undo_does_not_return_after_switching_tabs_back() {
        let scope = test_settings_scope("ux-tab-undo");
        let mut app = scope.app(test_root("ux-tab-undo"), 50, "report".into());
        app.shell.runtime.ignore_case = false;
        app.relax_filter(FilterValue::Case(true));
        app.create_new_tab();
        app.switch_to_tab_index(0);
        assert!(app.shell.ui.search_assist.undo.is_none());
        assert!(app.shell.runtime.ignore_case);
    }

    #[test]
    fn ux_advice_is_bounded_without_truncating_actual_input() {
        let query = format!("{} neme:x", "日".repeat(MAX_ADVICE_BYTES));
        assert!(query_advice(&query).is_none());
        let scope = test_settings_scope("ux-long-query");
        let mut app = scope.app(test_root("ux-long-query"), 50, query.clone());
        app.sync_search_assist(Instant::now());
        assert!(app.shell.ui.search_assist.advice.is_none());
        assert_eq!(app.shell.runtime.query_state.query, query);
    }

    #[test]
    fn ux_relaxation_esc_keeps_filters_and_undo_only_restores_affected_setting() {
        let scope = test_settings_scope("ux-undo");
        let mut app = scope.app(test_root("ux-undo"), 50, "report".into());
        app.shell.runtime.ignore_case = false;
        app.relax_filter(FilterValue::Case(true));
        assert!(app.shell.runtime.ignore_case);
        app.clear_query_and_selection();
        assert!(app.shell.runtime.ignore_case);
        assert!(app.shell.runtime.query_state.query.is_empty());
        app.shell.runtime.use_regex = true;
        app.undo_filter_relaxation();
        assert!(!app.shell.runtime.ignore_case);
        assert!(app.shell.runtime.use_regex);
        assert!(app.shell.runtime.query_state.query.is_empty());
        assert!(app.shell.ui.search_assist.undo.is_none());
    }

    #[test]
    fn ux_undo_invalidated_by_manual_change_and_context() {
        let scope = test_settings_scope("ux-undo-context");
        let mut app = scope.app(test_root("ux-undo-context"), 50, "report".into());
        app.shell.runtime.ignore_case = false;
        app.relax_filter(FilterValue::Case(true));
        app.manual_filter_changed(FilterValue::Regex(true));
        assert!(app.shell.ui.search_assist.undo.is_some());
        app.manual_filter_changed(FilterValue::Case(false));
        assert!(app.shell.ui.search_assist.undo.is_none());
        app.shell.runtime.ignore_case = false;
        app.relax_filter(FilterValue::Case(true));
        app.shell.runtime.root = test_root("new-context");
        app.sync_search_assist(Instant::now());
        assert!(app.shell.ui.search_assist.undo.is_none());
    }

    #[test]
    fn ux_contract_same_value_preset_invalidates_filter_undo() {
        use crate::search_catalog::{PresetEntryType, PresetSortMode, PresetSource, SearchPreset};
        let scope = test_settings_scope("ux-preset-undo");
        let root = test_root("ux-preset-undo-root");
        std::fs::create_dir_all(&root).unwrap();
        let mut app = scope.app(root.clone(), 50, "report".into());
        app.shell.runtime.use_filelist = false;
        app.shell.runtime.include_files = true;
        app.shell.runtime.include_dirs = true;
        app.shell.runtime.ignore_case = false;
        app.relax_filter(FilterValue::Case(true));
        assert!(app.shell.ui.search_assist.undo.is_some());
        app.shell
            .features
            .presets
            .catalog
            .save_preset(SearchPreset {
                name: "Same conditions".into(),
                root_name: None,
                root_path: root.clone(),
                query: "report".into(),
                entry_type: PresetEntryType::All,
                source: PresetSource::Walker,
                regex: app.shell.runtime.use_regex,
                ignore_case: true,
                ignore_enabled: app.shell.ui.ignore_list_enabled,
                sort: PresetSortMode::Score,
                max_depth: app.shell.runtime.max_depth,
                follow_links: app.shell.runtime.follow_links,
                extra: Default::default(),
            })
            .unwrap();
        app.shell.features.presets.picker.open = true;
        app.refresh_preset_picker_matches();
        app.apply_selected_preset();
        assert!(
            !app.shell.features.presets.picker.open,
            "preset actually applied"
        );
        assert!(app.shell.ui.search_assist.undo.is_none());
        app.undo_filter_relaxation();
        assert!(
            app.shell.runtime.ignore_case,
            "old undo must not revert the preset"
        );
        assert_eq!(app.shell.runtime.root, root);
        assert_eq!(app.shell.runtime.query_state.query, "report");
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn ux_repair_preserves_query_operators_focus_and_ime() {
        let scope = test_settings_scope("ux-repair");
        let mut app = scope.app(
            test_root("ux-repair"),
            50,
            "日本語 !neme:report ext:rs".into(),
        );
        app.sync_search_assist(Instant::now());
        let Some(QueryAdvice::Typo { range, replacement }) =
            app.shell.ui.search_assist.advice.clone()
        else {
            panic!("advice");
        };
        app.shell.ui.ime_composition_active = true;
        app.repair_query_prefix(range.clone(), replacement);
        assert!(app.shell.runtime.query_state.query.contains("neme:"));
        app.shell.ui.ime_composition_active = false;
        app.repair_query_prefix(range, replacement);
        assert_eq!(
            app.shell.runtime.query_state.query,
            "日本語 !name:report ext:rs"
        );
        assert!(app.shell.ui.focus_query_requested);
        assert!(app.shell.ui.query_cursor_to_end_requested);
    }

    #[test]
    fn ux_worker_error_is_query_owned_and_stale_response_does_not_replace_it() {
        use crate::app::{ResultSortMode, ResultSortScope, SearchResponse};
        let scope = test_settings_scope("ux-error");
        let mut app = scope.app(test_root("ux-error"), 50, "[".into());
        app.shell.search.set_pending_request_id(Some(10));
        let response = |request_id, text: &str| SearchResponse {
            request_id,
            results: vec![],
            total_match_count: 0,
            sort_mode: ResultSortMode::Score,
            sort_scope: ResultSortScope::ShownResults,
            error: Some(text.into()),
        };
        assert!(!crate::app::result_reducer::apply_active_search_response(
            &mut app,
            response(9, "old")
        ));
        assert!(app.current_search_error().is_none());
        assert!(crate::app::result_reducer::apply_active_search_response(
            &mut app,
            response(10, "invalid regex")
        ));
        assert_eq!(app.current_search_error(), Some("invalid regex"));
        app.shell.runtime.query_state.query = "valid".into();
        assert!(app.current_search_error().is_none());
    }

    #[test]
    fn ux_assistance_renders_narrow_frames_without_changing_query() {
        let scope = test_settings_scope("ux-frames");
        let mut app = scope.app(test_root("ux-frames"), 50, "neme:report".into());
        let ctx = egui::Context::default();
        for query in ["neme:report", "name:", "nothing"] {
            app.shell.runtime.query_state.query = query.into();
            settled(&mut app);
            for _ in 0..2 {
                let _ = ctx.run_ui(
                    egui::RawInput {
                        screen_rect: Some(egui::Rect::from_min_size(
                            egui::Pos2::ZERO,
                            egui::vec2(640.0, 400.0),
                        )),
                        ..Default::default()
                    },
                    |ui| app.run_ui_frame(ui),
                );
            }
            assert_eq!(app.shell.runtime.query_state.query, query);
        }
    }

    #[test]
    fn ux_query_advice_colon_typo_paste_and_unicode_offsets() {
        let query = "日本語 !neme:report ext:rs";
        let Some(QueryAdvice::Typo { range, replacement }) = query_advice(query) else {
            panic!("expected optional name repair");
        };
        let mut repaired = query.to_string();
        repaired.replace_range(range, replacement);
        assert_eq!(repaired, "日本語 !name:report ext:rs");
        assert!(query_advice("neme").is_none());
        assert!(matches!(
            query_advice("neme:"),
            Some(QueryAdvice::Typo { .. })
        ));
        assert!(matches!(
            query_advice("naem:x"),
            Some(QueryAdvice::Typo { .. })
        ));
    }

    #[test]
    fn ux_query_advice_known_empty_field_waits_for_value() {
        assert_eq!(query_advice("!name:"), Some(QueryAdvice::Pending("name")));
        assert!(query_advice("name:日本語").is_none());
    }

    #[test]
    fn ux_query_advice_preserves_paths_urls_literals_and_regex_colons() {
        for query in [
            r"C:\work",
            "https://host",
            "neme://host",
            "'neme:x",
            "(?:name:x)",
            "path:C:/work",
            "name:neme:x",
            "other:x",
            "時計:十二時",
        ] {
            assert!(query_advice(query).is_none(), "{query}");
        }
    }
}

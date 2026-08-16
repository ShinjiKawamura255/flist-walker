use super::{FlistWalkerApp, ResultSortMode};
use crate::path_utils::path_key;
use crate::search_catalog::{validate_catalog_name, PresetSortMode, PresetSource, SearchPreset};
use eframe::egui;
use fuzzy_matcher::skim::SkimMatcherV2;
use fuzzy_matcher::FuzzyMatcher;

fn preset_name_score(query: &str, candidate: &str, catalog_index: usize) -> Option<i64> {
    if query.trim().is_empty() {
        return Some(-(catalog_index as i64));
    }

    let matcher = SkimMatcherV2::default();
    matcher.fuzzy_match(candidate, query).or_else(|| {
        let query_lower = query.to_ascii_lowercase();
        let candidate_lower = candidate.to_ascii_lowercase();
        candidate_lower
            .contains(&query_lower)
            .then_some((query_lower.len() as i64) * 100 - catalog_index as i64)
    })
}

impl FlistWalkerApp {
    pub(in crate::app) const PRESET_PICKER_QUERY_ID: &'static str = "preset-picker-query";

    pub(in crate::app) fn open_preset_picker(&mut self, ctx: &egui::Context) {
        let restore_query_focus =
            ctx.memory(|memory| memory.has_focus(self.shell.ui.query_input_id));
        let picker = &mut self.shell.features.presets.picker;
        picker.open = true;
        picker.restore_query_focus = restore_query_focus;
        picker.query.clear();
        picker.error.clear();
        picker.editor = Default::default();
        picker.focus_requested = true;
        self.refresh_preset_picker_matches();
        ctx.memory_mut(|memory| {
            memory.request_focus(egui::Id::new(Self::PRESET_PICKER_QUERY_ID));
        });
        self.request_preset_catalog_load();
    }

    pub(in crate::app) fn close_preset_picker(&mut self) {
        let picker = &mut self.shell.features.presets.picker;
        let restore_query_focus = picker.restore_query_focus;
        picker.open = false;
        picker.restore_query_focus = false;
        picker.query.clear();
        picker.matched_catalog_indices.clear();
        picker.selected_match = None;
        picker.focus_requested = false;
        picker.error.clear();
        picker.editor = Default::default();
        if restore_query_focus {
            self.request_focus_query();
            self.clear_unfocus_query_request();
        }
    }

    fn request_preset_catalog_load(&mut self) {
        let request_id = self.shell.worker_bus.catalog.begin_request();
        if self
            .shell
            .worker_bus
            .catalog
            .tx
            .send(super::CatalogRequest {
                request_id,
                kind: super::CatalogRequestKind::Load,
            })
            .is_err()
        {
            self.shell.worker_bus.catalog.clear_request();
            self.shell.features.presets.picker.error =
                "Preset catalog worker is unavailable".to_string();
        }
    }

    pub(in crate::app) fn poll_catalog_response(&mut self) {
        while let Ok(response) = self.shell.worker_bus.catalog.rx.try_recv() {
            if self.shell.worker_bus.catalog.pending_request_id != Some(response.request_id) {
                continue;
            }
            self.shell.worker_bus.catalog.clear_request();
            let pending_saved_name = self
                .shell
                .features
                .presets
                .picker
                .editor
                .pending_saved_name
                .take();
            match response.result {
                Ok(catalog) => {
                    self.shell.features.presets.catalog = catalog;
                    self.shell.features.presets.picker.error.clear();
                    if let Some(saved_name) = pending_saved_name {
                        self.shell.features.presets.picker.editor = Default::default();
                        self.shell.features.presets.picker.query.clear();
                        self.refresh_preset_picker_matches();
                        self.select_preset_picker_name(&saved_name);
                        self.shell.features.presets.picker.focus_requested = true;
                        self.set_notice(format!("Saved preset: {saved_name}"));
                        continue;
                    }
                    self.refresh_preset_picker_matches();
                }
                Err(error) => {
                    if pending_saved_name.is_some() {
                        self.shell.features.presets.picker.editor.error = error;
                        continue;
                    }
                    self.shell.features.presets.picker.error = error;
                    self.shell
                        .features
                        .presets
                        .picker
                        .matched_catalog_indices
                        .clear();
                    self.shell.features.presets.picker.selected_match = None;
                }
            }
        }
    }

    pub(in crate::app) fn refresh_preset_picker_matches(&mut self) {
        let query = self.shell.features.presets.picker.query.trim();
        let mut scored = self
            .shell
            .features
            .presets
            .catalog
            .presets
            .iter()
            .enumerate()
            .filter_map(|(catalog_index, preset)| {
                preset_name_score(query, &preset.name, catalog_index)
                    .map(|score| (catalog_index, score))
            })
            .collect::<Vec<_>>();
        scored.sort_by(|left, right| right.1.cmp(&left.1).then_with(|| left.0.cmp(&right.0)));
        let picker = &mut self.shell.features.presets.picker;
        picker.matched_catalog_indices = scored
            .into_iter()
            .map(|(catalog_index, _)| catalog_index)
            .collect();
        picker.selected_match = (!picker.matched_catalog_indices.is_empty()).then_some(0);
    }

    pub(in crate::app) fn move_preset_picker_selection(&mut self, delta: isize) {
        let picker = &mut self.shell.features.presets.picker;
        let count = picker.matched_catalog_indices.len();
        if count == 0 {
            picker.selected_match = None;
            return;
        }
        let current = picker.selected_match.unwrap_or(0) as isize;
        picker.selected_match = Some((current + delta).rem_euclid(count as isize) as usize);
    }

    pub(in crate::app) fn select_preset_picker_match(&mut self, match_index: usize) {
        if match_index
            < self
                .shell
                .features
                .presets
                .picker
                .matched_catalog_indices
                .len()
        {
            self.shell.features.presets.picker.selected_match = Some(match_index);
        }
    }

    fn select_preset_picker_name(&mut self, name: &str) {
        let picker = &mut self.shell.features.presets.picker;
        picker.selected_match = picker
            .matched_catalog_indices
            .iter()
            .position(|catalog_index| {
                self.shell.features.presets.catalog.presets[*catalog_index]
                    .name
                    .eq_ignore_ascii_case(name)
            });
    }

    fn selected_preset(&self) -> Option<SearchPreset> {
        let picker = &self.shell.features.presets.picker;
        let catalog_index = picker
            .selected_match
            .and_then(|match_index| picker.matched_catalog_indices.get(match_index))?;
        self.shell
            .features
            .presets
            .catalog
            .presets
            .get(*catalog_index)
            .cloned()
    }

    pub(in crate::app) fn start_selected_preset_edit(&mut self) {
        if self.shell.worker_bus.catalog.in_progress {
            return;
        }
        let Some(preset) = self.selected_preset() else {
            return;
        };
        self.shell.features.presets.picker.editor = super::state::PresetEditorState {
            open: true,
            original_name: preset.name.clone(),
            name: preset.name,
            root_name: preset.root_name,
            root_path: preset.root_path.display().to_string(),
            query: preset.query,
            entry_type: preset.entry_type,
            source: preset.source,
            regex: preset.regex,
            ignore_case: preset.ignore_case,
            ignore_enabled: preset.ignore_enabled,
            sort: preset.sort,
            extra: preset.extra,
            focus_requested: true,
            error: String::new(),
            pending_saved_name: None,
        };
    }

    pub(in crate::app) fn cancel_preset_edit(&mut self) {
        if self
            .shell
            .features
            .presets
            .picker
            .editor
            .pending_saved_name
            .is_some()
        {
            return;
        }
        self.shell.features.presets.picker.editor = Default::default();
        self.shell.features.presets.picker.focus_requested = true;
    }

    pub(in crate::app) fn request_save_preset_edit(&mut self) {
        if self.shell.worker_bus.catalog.in_progress {
            self.shell.features.presets.picker.editor.error =
                "Catalog update already in progress".to_string();
            return;
        }
        let editor = &self.shell.features.presets.picker.editor;
        if !editor.open {
            return;
        }
        let name = match validate_catalog_name(&editor.name) {
            Ok(name) => name,
            Err(error) => {
                self.shell.features.presets.picker.editor.error = error.to_string();
                return;
            }
        };
        let root_path = editor.root_path.trim();
        if root_path.is_empty() {
            self.shell.features.presets.picker.editor.error =
                "Preset root must not be empty".to_string();
            return;
        }
        let root_path = std::path::PathBuf::from(root_path);
        if !root_path.is_absolute() {
            self.shell.features.presets.picker.editor.error =
                "Preset root must be an absolute path".to_string();
            return;
        }
        let preset = SearchPreset {
            name: name.clone(),
            root_name: editor.root_name.clone(),
            root_path,
            query: editor.query.clone(),
            entry_type: editor.entry_type,
            source: editor.source,
            regex: editor.regex,
            ignore_case: editor.ignore_case,
            ignore_enabled: editor.ignore_enabled,
            sort: editor.sort,
            extra: editor.extra.clone(),
        };
        let original_name = editor.original_name.clone();
        let request_id = self.shell.worker_bus.catalog.begin_request();
        self.shell.features.presets.picker.editor.pending_saved_name = Some(name);
        self.shell.features.presets.picker.editor.error.clear();
        if self
            .shell
            .worker_bus
            .catalog
            .tx
            .send(super::CatalogRequest {
                request_id,
                kind: super::CatalogRequestKind::ReplacePreset {
                    original_name,
                    preset,
                },
            })
            .is_err()
        {
            self.shell.worker_bus.catalog.clear_request();
            self.shell.features.presets.picker.editor.pending_saved_name = None;
            self.shell.features.presets.picker.editor.error =
                "Preset catalog worker is unavailable".to_string();
        }
    }

    pub(in crate::app) fn apply_selected_preset(&mut self) {
        if self.shell.worker_bus.catalog.in_progress {
            return;
        }
        let Some(preset) = self.selected_preset() else {
            return;
        };
        let new_root = self
            .shell
            .features
            .presets
            .catalog
            .resolve_preset_root(&preset);
        let (include_files, include_dirs) = preset.entry_type.include_flags();
        let use_filelist = !matches!(preset.source, PresetSource::Walker);
        let requires_reindex = path_key(&new_root) != path_key(&self.shell.runtime.root)
            || self.shell.runtime.use_filelist != use_filelist
            || self.shell.runtime.include_files != include_files
            || self.shell.runtime.include_dirs != include_dirs;

        self.close_preset_picker();
        self.shell.runtime.query_state.query = preset.query;
        self.shell.runtime.use_filelist = use_filelist;
        self.shell.runtime.use_regex = preset.regex;
        self.shell.runtime.ignore_case = preset.ignore_case;
        self.shell.runtime.include_files = include_files;
        self.shell.runtime.include_dirs = include_dirs;
        self.shell.ui.ignore_list_enabled = preset.ignore_enabled;
        if path_key(&new_root) != path_key(&self.shell.runtime.root) {
            self.apply_root_change_direct(new_root);
        } else if requires_reindex {
            self.request_index_refresh();
        } else {
            self.invalidate_result_sort(false);
            self.update_results();
        }
        self.set_result_sort_mode(runtime_sort_mode(preset.sort));
        self.sync_active_tab_state();
        self.set_notice(format!("Applied preset: {}", preset.name));
    }

    #[cfg(test)]
    pub(in crate::app) fn preset_picker_match_names(&self) -> Vec<&str> {
        self.shell
            .features
            .presets
            .picker
            .matched_catalog_indices
            .iter()
            .filter_map(|index| {
                self.shell
                    .features
                    .presets
                    .catalog
                    .presets
                    .get(*index)
                    .map(|preset| preset.name.as_str())
            })
            .collect()
    }
}

fn runtime_sort_mode(value: PresetSortMode) -> ResultSortMode {
    match value {
        PresetSortMode::Score => ResultSortMode::Score,
        PresetSortMode::NameAsc => ResultSortMode::NameAsc,
        PresetSortMode::NameDesc => ResultSortMode::NameDesc,
        PresetSortMode::ModifiedDesc => ResultSortMode::ModifiedDesc,
        PresetSortMode::ModifiedAsc => ResultSortMode::ModifiedAsc,
        PresetSortMode::CreatedDesc => ResultSortMode::CreatedDesc,
        PresetSortMode::CreatedAsc => ResultSortMode::CreatedAsc,
        PresetSortMode::SizeDesc => ResultSortMode::SizeDesc,
        PresetSortMode::SizeAsc => ResultSortMode::SizeAsc,
    }
}

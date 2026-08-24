use super::{FlistWalkerApp, ResultSortMode, ResultSortScope};
use crate::path_utils::{normalize_path_for_display, normalize_windows_path_buf, path_key};
use crate::search_catalog::{
    validate_catalog_name, NamedRoot, PresetSortMode, PresetSource, SearchPreset,
};
use eframe::egui;
use fuzzy_matcher::skim::SkimMatcherV2;
use fuzzy_matcher::FuzzyMatcher;
use std::path::PathBuf;

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
        picker.named_roots = Default::default();
        picker.focus_requested = true;
        picker.confirm_delete = false;
        picker.pending_deleted_name = None;
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
        picker.confirm_delete = false;
        picker.pending_deleted_name = None;
        picker.editor = Default::default();
        picker.named_roots = Default::default();
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
            let pending_named_root_operation = self
                .shell
                .features
                .presets
                .picker
                .named_roots
                .pending_operation
                .take();
            let pending_saved_name = self
                .shell
                .features
                .presets
                .picker
                .editor
                .pending_saved_name
                .take();
            let pending_deleted_name = self
                .shell
                .features
                .presets
                .picker
                .pending_deleted_name
                .take();
            match response.result {
                Ok(catalog) => {
                    self.shell.features.presets.catalog = catalog;
                    self.shell.features.presets.picker.error.clear();
                    if let Some(operation) = pending_named_root_operation {
                        self.complete_named_root_operation(operation);
                        continue;
                    }
                    if let Some(saved_name) = pending_saved_name {
                        self.shell.features.presets.picker.editor = Default::default();
                        self.shell.features.presets.picker.query.clear();
                        self.refresh_preset_picker_matches();
                        self.select_preset_picker_name(&saved_name);
                        self.shell.features.presets.picker.focus_requested = true;
                        self.set_notice(format!("Saved preset: {saved_name}"));
                        continue;
                    }
                    if let Some(deleted_name) = pending_deleted_name {
                        self.shell.features.presets.picker.confirm_delete = false;
                        self.refresh_preset_picker_matches();
                        self.shell.features.presets.picker.focus_requested = true;
                        self.set_notice(format!("Deleted preset: {deleted_name}"));
                        continue;
                    }
                    self.refresh_preset_picker_matches();
                }
                Err(error) => {
                    if let Some(operation) = pending_named_root_operation {
                        match operation {
                            super::state::PendingNamedRootOperation::Save { .. } => {
                                self.shell.features.presets.picker.named_roots.editor.error = error;
                            }
                            super::state::PendingNamedRootOperation::Delete { .. } => {
                                self.shell.features.presets.picker.named_roots.error = error;
                            }
                        }
                        continue;
                    }
                    if pending_saved_name.is_some() {
                        self.shell.features.presets.picker.editor.error = error;
                        continue;
                    }
                    if pending_deleted_name.is_some() {
                        self.shell.features.presets.picker.error = error;
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

    fn complete_named_root_operation(
        &mut self,
        operation: super::state::PendingNamedRootOperation,
    ) {
        let manager = &mut self.shell.features.presets.picker.named_roots;
        manager.error.clear();
        manager.confirm_delete = false;
        match operation {
            super::state::PendingNamedRootOperation::Save {
                original_name,
                saved_name,
            } => {
                if let Some(original_name) = original_name {
                    let preset_editor = &mut self.shell.features.presets.picker.editor;
                    if preset_editor
                        .root_name
                        .as_deref()
                        .is_some_and(|name| name.eq_ignore_ascii_case(&original_name))
                    {
                        preset_editor.root_name = Some(saved_name.clone());
                    }
                }
                manager.editor = Default::default();
                manager.selected_index = self
                    .shell
                    .features
                    .presets
                    .catalog
                    .named_roots
                    .iter()
                    .position(|root| root.name.eq_ignore_ascii_case(&saved_name));
                self.set_notice(format!("Saved named root: {saved_name}"));
            }
            super::state::PendingNamedRootOperation::Delete { name } => {
                let preset_editor = &mut self.shell.features.presets.picker.editor;
                if preset_editor
                    .root_name
                    .as_deref()
                    .is_some_and(|candidate| candidate.eq_ignore_ascii_case(&name))
                {
                    preset_editor.root_name = None;
                }
                let count = self.shell.features.presets.catalog.named_roots.len();
                manager.selected_index = if count == 0 {
                    None
                } else {
                    Some(manager.selected_index.unwrap_or(0).min(count - 1))
                };
                self.set_notice(format!("Deleted named root: {name}"));
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
        picker.confirm_delete = false;
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
        picker.confirm_delete = false;
        picker.error.clear();
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
            self.shell.features.presets.picker.confirm_delete = false;
            self.shell.features.presets.picker.error.clear();
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
            root_path: normalize_path_for_display(&preset.root_path),
            query: preset.query,
            entry_type: preset.entry_type,
            source: preset.source,
            regex: preset.regex,
            ignore_case: preset.ignore_case,
            ignore_enabled: preset.ignore_enabled,
            sort: preset.sort,
            max_depth: preset.max_depth,
            extra: preset.extra,
            focus_requested: true,
            error: String::new(),
            pending_saved_name: None,
        };
        self.shell.features.presets.picker.confirm_delete = false;
    }

    pub(in crate::app) fn start_add_preset(&mut self) {
        if self.shell.worker_bus.catalog.in_progress {
            return;
        }
        let runtime = &self.shell.runtime;
        self.shell.features.presets.picker.editor = super::state::PresetEditorState {
            open: true,
            original_name: String::new(),
            name: String::new(),
            root_name: None,
            root_path: normalize_path_for_display(&runtime.root),
            query: runtime.query_state.query.clone(),
            entry_type: preset_entry_type(runtime.include_files, runtime.include_dirs),
            source: if runtime.use_filelist {
                PresetSource::Auto
            } else {
                PresetSource::Walker
            },
            regex: runtime.use_regex,
            ignore_case: runtime.ignore_case,
            ignore_enabled: self.shell.ui.ignore_list_enabled,
            sort: preset_sort_mode(runtime.result_sort_mode),
            max_depth: runtime.max_depth,
            extra: Default::default(),
            focus_requested: true,
            error: String::new(),
            pending_saved_name: None,
        };
        self.shell.features.presets.picker.confirm_delete = false;
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

    pub(in crate::app) fn browse_for_preset_editor_root(&mut self) {
        if self.shell.worker_bus.catalog.in_progress
            || !self.shell.features.presets.picker.editor.open
        {
            return;
        }
        let input = self.shell.features.presets.picker.editor.root_path.clone();
        match self.select_root_for_path_input(&input) {
            Ok(Some(path)) => {
                let editor = &mut self.shell.features.presets.picker.editor;
                editor.root_path = normalize_path_for_display(&path);
                editor.error.clear();
            }
            Ok(None) => {}
            Err(error) => {
                self.shell.features.presets.picker.editor.error = format!("Browse failed: {error}");
            }
        }
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
        let root_path = normalize_windows_path_buf(std::path::PathBuf::from(root_path));
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
            max_depth: editor.max_depth,
            extra: editor.extra.clone(),
        };
        let original_name = editor.original_name.clone();
        let kind = if original_name.is_empty() {
            super::CatalogRequestKind::AddPreset { preset }
        } else {
            super::CatalogRequestKind::ReplacePreset {
                original_name,
                preset,
            }
        };
        let request_id = self.shell.worker_bus.catalog.begin_request();
        self.shell.features.presets.picker.editor.pending_saved_name = Some(name);
        self.shell.features.presets.picker.editor.error.clear();
        if self
            .shell
            .worker_bus
            .catalog
            .tx
            .send(super::CatalogRequest { request_id, kind })
            .is_err()
        {
            self.shell.worker_bus.catalog.clear_request();
            self.shell.features.presets.picker.editor.pending_saved_name = None;
            self.shell.features.presets.picker.editor.error =
                "Preset catalog worker is unavailable".to_string();
        }
    }

    pub(in crate::app) fn start_selected_preset_delete(&mut self) {
        if self.shell.worker_bus.catalog.in_progress || self.selected_preset().is_none() {
            return;
        }
        let picker = &mut self.shell.features.presets.picker;
        picker.confirm_delete = true;
        picker.error.clear();
    }

    pub(in crate::app) fn cancel_delete_preset(&mut self) {
        if self.shell.worker_bus.catalog.in_progress {
            return;
        }
        let picker = &mut self.shell.features.presets.picker;
        picker.confirm_delete = false;
        picker.error.clear();
        picker.focus_requested = true;
    }

    pub(in crate::app) fn confirm_delete_preset(&mut self) {
        if self.shell.worker_bus.catalog.in_progress {
            return;
        }
        let Some(preset) = self.selected_preset() else {
            self.shell.features.presets.picker.confirm_delete = false;
            return;
        };
        let name = preset.name;
        let request_id = self.shell.worker_bus.catalog.begin_request();
        self.shell.features.presets.picker.pending_deleted_name = Some(name.clone());
        self.shell.features.presets.picker.error.clear();
        if self
            .shell
            .worker_bus
            .catalog
            .tx
            .send(super::CatalogRequest {
                request_id,
                kind: super::CatalogRequestKind::RemovePreset { name },
            })
            .is_err()
        {
            self.shell.worker_bus.catalog.clear_request();
            let picker = &mut self.shell.features.presets.picker;
            picker.pending_deleted_name = None;
            picker.error = "Preset catalog worker is unavailable".to_string();
        }
    }

    pub(in crate::app) fn open_named_root_manager(&mut self) {
        if self.shell.worker_bus.catalog.in_progress {
            return;
        }
        let manager = &mut self.shell.features.presets.picker.named_roots;
        manager.open = true;
        manager.selected_index =
            (!self.shell.features.presets.catalog.named_roots.is_empty()).then_some(0);
        manager.confirm_delete = false;
        manager.error.clear();
        manager.editor = Default::default();
    }

    pub(in crate::app) fn close_named_root_manager(&mut self) {
        if self.shell.worker_bus.catalog.in_progress {
            return;
        }
        self.shell.features.presets.picker.named_roots = Default::default();
        if self.shell.features.presets.picker.editor.open {
            self.shell.features.presets.picker.editor.focus_requested = true;
        } else {
            self.shell.features.presets.picker.focus_requested = true;
        }
    }

    pub(in crate::app) fn move_named_root_selection(&mut self, delta: isize) {
        let manager = &mut self.shell.features.presets.picker.named_roots;
        let count = self.shell.features.presets.catalog.named_roots.len();
        if count == 0 {
            manager.selected_index = None;
            return;
        }
        let current = manager.selected_index.unwrap_or(0) as isize;
        manager.selected_index = Some((current + delta).rem_euclid(count as isize) as usize);
        manager.confirm_delete = false;
        manager.error.clear();
    }

    fn selected_named_root(&self) -> Option<NamedRoot> {
        let manager = &self.shell.features.presets.picker.named_roots;
        let index = manager.selected_index?;
        self.shell
            .features
            .presets
            .catalog
            .named_roots
            .get(index)
            .cloned()
    }

    pub(in crate::app) fn start_add_named_root(&mut self) {
        if self.shell.worker_bus.catalog.in_progress {
            return;
        }
        let path = normalize_path_for_display(&self.shell.runtime.root);
        self.shell.features.presets.picker.named_roots.editor =
            super::state::NamedRootEditorState {
                open: true,
                original_name: None,
                name: String::new(),
                path,
                focus_requested: true,
                error: String::new(),
            };
        self.shell
            .features
            .presets
            .picker
            .named_roots
            .confirm_delete = false;
    }

    pub(in crate::app) fn start_selected_named_root_edit(&mut self) {
        if self.shell.worker_bus.catalog.in_progress {
            return;
        }
        let Some(root) = self.selected_named_root() else {
            return;
        };
        self.shell.features.presets.picker.named_roots.editor =
            super::state::NamedRootEditorState {
                open: true,
                original_name: Some(root.name.clone()),
                name: root.name,
                path: normalize_path_for_display(&root.path),
                focus_requested: true,
                error: String::new(),
            };
        self.shell
            .features
            .presets
            .picker
            .named_roots
            .confirm_delete = false;
    }

    pub(in crate::app) fn cancel_named_root_edit(&mut self) {
        if self.shell.worker_bus.catalog.in_progress {
            return;
        }
        self.shell.features.presets.picker.named_roots.editor = Default::default();
    }

    pub(in crate::app) fn browse_for_named_root_editor_path(&mut self) {
        if self.shell.worker_bus.catalog.in_progress
            || !self.shell.features.presets.picker.named_roots.editor.open
        {
            return;
        }
        let input = self
            .shell
            .features
            .presets
            .picker
            .named_roots
            .editor
            .path
            .clone();
        match self.select_root_for_path_input(&input) {
            Ok(Some(path)) => {
                let editor = &mut self.shell.features.presets.picker.named_roots.editor;
                editor.path = normalize_path_for_display(&path);
                editor.error.clear();
            }
            Ok(None) => {}
            Err(error) => {
                self.shell.features.presets.picker.named_roots.editor.error =
                    format!("Browse failed: {error}");
            }
        }
    }

    pub(in crate::app) fn request_save_named_root(&mut self) {
        if self.shell.worker_bus.catalog.in_progress {
            return;
        }
        let editor = &self.shell.features.presets.picker.named_roots.editor;
        if !editor.open {
            return;
        }
        let name = match validate_catalog_name(&editor.name) {
            Ok(name) => name,
            Err(error) => {
                self.shell.features.presets.picker.named_roots.editor.error = error.to_string();
                return;
            }
        };
        let path = normalize_windows_path_buf(std::path::PathBuf::from(editor.path.trim()));
        if editor.path.trim().is_empty() {
            self.shell.features.presets.picker.named_roots.editor.error =
                "Named root path must not be empty".to_string();
            return;
        }
        if !path.is_absolute() {
            self.shell.features.presets.picker.named_roots.editor.error =
                "Named root path must be an absolute path".to_string();
            return;
        }
        let original_name = editor.original_name.clone();
        let kind = match original_name.as_ref() {
            Some(original_name) => super::CatalogRequestKind::ReplaceNamedRoot {
                original_name: original_name.clone(),
                name: name.clone(),
                path,
            },
            None => super::CatalogRequestKind::AddNamedRoot {
                name: name.clone(),
                path,
            },
        };
        let request_id = self.shell.worker_bus.catalog.begin_request();
        self.shell
            .features
            .presets
            .picker
            .named_roots
            .pending_operation = Some(super::state::PendingNamedRootOperation::Save {
            original_name,
            saved_name: name,
        });
        self.shell
            .features
            .presets
            .picker
            .named_roots
            .editor
            .error
            .clear();
        if self
            .shell
            .worker_bus
            .catalog
            .tx
            .send(super::CatalogRequest { request_id, kind })
            .is_err()
        {
            self.shell.worker_bus.catalog.clear_request();
            let manager = &mut self.shell.features.presets.picker.named_roots;
            manager.pending_operation = None;
            manager.editor.error = "Preset catalog worker is unavailable".to_string();
        }
    }

    pub(in crate::app) fn start_selected_named_root_delete(&mut self) {
        if self.shell.worker_bus.catalog.in_progress || self.selected_named_root().is_none() {
            return;
        }
        let manager = &mut self.shell.features.presets.picker.named_roots;
        manager.confirm_delete = true;
        manager.error.clear();
    }

    pub(in crate::app) fn cancel_delete_named_root(&mut self) {
        if self.shell.worker_bus.catalog.in_progress {
            return;
        }
        let manager = &mut self.shell.features.presets.picker.named_roots;
        manager.confirm_delete = false;
        manager.error.clear();
    }

    pub(in crate::app) fn confirm_delete_named_root(&mut self) {
        if self.shell.worker_bus.catalog.in_progress {
            return;
        }
        let Some(root) = self.selected_named_root() else {
            return;
        };
        let request_id = self.shell.worker_bus.catalog.begin_request();
        self.shell
            .features
            .presets
            .picker
            .named_roots
            .pending_operation = Some(super::state::PendingNamedRootOperation::Delete {
            name: root.name.clone(),
        });
        if self
            .shell
            .worker_bus
            .catalog
            .tx
            .send(super::CatalogRequest {
                request_id,
                kind: super::CatalogRequestKind::RemoveNamedRoot { name: root.name },
            })
            .is_err()
        {
            self.shell.worker_bus.catalog.clear_request();
            let manager = &mut self.shell.features.presets.picker.named_roots;
            manager.pending_operation = None;
            manager.error = "Preset catalog worker is unavailable".to_string();
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
        self.close_preset_picker();
        self.apply_preset_runtime_transition(&preset, new_root);
        self.request_query_cursor_to_end();
        self.sync_active_tab_state();
        self.set_notice(format!("Applied preset: {}", preset.name));
    }

    fn apply_preset_runtime_transition(&mut self, preset: &SearchPreset, new_root: PathBuf) {
        let (include_files, include_dirs) = preset.entry_type.include_flags();
        let use_filelist = !matches!(preset.source, PresetSource::Walker);
        let root_changed = path_key(&new_root) != path_key(&self.shell.runtime.root);
        let requires_reindex = root_changed
            || self.shell.runtime.use_filelist != use_filelist
            || self.shell.runtime.include_files != include_files
            || self.shell.runtime.include_dirs != include_dirs
            || self.shell.runtime.max_depth != preset.max_depth;
        let sort_mode = runtime_sort_mode(preset.sort);
        let sort_scope = self.shell.runtime.result_sort_scope;

        self.shell.runtime.query_state.query = preset.query.clone();
        self.shell.runtime.use_filelist = use_filelist;
        self.shell.runtime.use_regex = preset.regex;
        self.shell.runtime.ignore_case = preset.ignore_case;
        self.shell.runtime.include_files = include_files;
        self.shell.runtime.include_dirs = include_dirs;
        self.shell.runtime.max_depth = preset.max_depth;
        self.shell.ui.ignore_list_enabled = preset.ignore_enabled;

        // Regression guard: preset-owned state must be committed before exactly one
        // reindex or filter/search transition. Do not dispatch search first or bypass
        // entry filters; paired tests cover stale responses, Ignore List, and sort state.
        if root_changed {
            self.apply_root_change_direct(new_root);
        } else if requires_reindex {
            self.request_index_refresh();
        } else {
            self.shell.worker_bus.sort.clear_request();
            self.shell.runtime.result_sort_mode = sort_mode;
            self.shell.runtime.result_sort_scope = sort_scope;
            self.apply_entry_filters(false);
            if self.shell.runtime.query_state.query.trim().is_empty()
                && sort_scope == ResultSortScope::ShownResults
                && sort_mode != ResultSortMode::Score
            {
                self.apply_result_sort(false);
            }
            return;
        }

        // Index refresh clears result sort state; restore the preset-owned mode and
        // the tab-owned scope before the refreshed index can resume searching.
        self.shell.runtime.result_sort_mode = sort_mode;
        self.shell.runtime.result_sort_scope = sort_scope;
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

fn preset_entry_type(
    include_files: bool,
    include_dirs: bool,
) -> crate::search_catalog::PresetEntryType {
    match (include_files, include_dirs) {
        (true, false) => crate::search_catalog::PresetEntryType::File,
        (false, true) => crate::search_catalog::PresetEntryType::Folder,
        _ => crate::search_catalog::PresetEntryType::All,
    }
}

fn preset_sort_mode(value: ResultSortMode) -> PresetSortMode {
    match value {
        ResultSortMode::Score => PresetSortMode::Score,
        ResultSortMode::NameAsc => PresetSortMode::NameAsc,
        ResultSortMode::NameDesc => PresetSortMode::NameDesc,
        ResultSortMode::ModifiedDesc => PresetSortMode::ModifiedDesc,
        ResultSortMode::ModifiedAsc => PresetSortMode::ModifiedAsc,
        ResultSortMode::CreatedDesc => PresetSortMode::CreatedDesc,
        ResultSortMode::CreatedAsc => PresetSortMode::CreatedAsc,
        ResultSortMode::SizeDesc => PresetSortMode::SizeDesc,
        ResultSortMode::SizeAsc => PresetSortMode::SizeAsc,
    }
}

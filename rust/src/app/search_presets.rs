use super::{CatalogMutation, CatalogRequest, FlistWalkerApp, ResultSortMode};
use crate::path_utils::path_key;
use crate::search_catalog::{PresetEntryType, PresetSortMode, PresetSource, SearchPreset};
use std::collections::BTreeMap;

impl FlistWalkerApp {
    pub(super) fn request_name_current_root(&mut self) {
        let name = self
            .shell
            .features
            .presets
            .root_name_input
            .trim()
            .to_string();
        if name.is_empty() {
            self.shell.features.presets.error = "Enter a root name".to_string();
            return;
        }
        self.send_catalog_mutation(
            CatalogMutation::AddNamedRoot {
                name: name.clone(),
                path: self.shell.runtime.root.clone(),
            },
            None,
        );
    }

    pub(super) fn request_save_current_preset(&mut self) {
        let name = self.shell.features.presets.name_input.trim().to_string();
        if name.is_empty() {
            self.shell.features.presets.error = "Enter a preset name".to_string();
            return;
        }
        let root_name = self
            .shell
            .features
            .presets
            .catalog
            .named_roots
            .iter()
            .find(|root| path_key(&root.path) == path_key(&self.shell.runtime.root))
            .map(|root| root.name.clone());
        let preset = SearchPreset {
            name: name.clone(),
            root_name,
            root_path: self.shell.runtime.root.clone(),
            query: self.shell.runtime.query_state.query.clone(),
            entry_type: match (
                self.shell.runtime.include_files,
                self.shell.runtime.include_dirs,
            ) {
                (true, false) => PresetEntryType::File,
                (false, true) => PresetEntryType::Folder,
                _ => PresetEntryType::All,
            },
            source: if self.shell.runtime.use_filelist {
                PresetSource::Auto
            } else {
                PresetSource::Walker
            },
            regex: self.shell.runtime.use_regex,
            ignore_case: self.shell.runtime.ignore_case,
            ignore_enabled: self.shell.ui.ignore_list_enabled,
            sort: preset_sort_mode(self.shell.runtime.result_sort_mode),
            extra: BTreeMap::new(),
        };
        self.send_catalog_mutation(CatalogMutation::SavePreset { preset }, Some(name));
    }

    pub(super) fn request_remove_selected_preset(&mut self) {
        let Some(name) = self.shell.features.presets.selected_name.clone() else {
            return;
        };
        self.send_catalog_mutation(CatalogMutation::RemovePreset { name }, None);
    }

    fn send_catalog_mutation(&mut self, mutation: CatalogMutation, select_after: Option<String>) {
        if self.shell.worker_bus.catalog.in_progress {
            self.shell.features.presets.error = "Catalog update already in progress".to_string();
            return;
        }
        let request_id = self.shell.worker_bus.catalog.begin_request();
        self.shell.features.presets.pending_selection = select_after;
        self.shell.features.presets.error.clear();
        if self
            .shell
            .worker_bus
            .catalog
            .tx
            .send(CatalogRequest {
                request_id,
                mutation,
            })
            .is_err()
        {
            self.shell.worker_bus.catalog.clear_request();
            self.shell.features.presets.error = "Catalog worker is unavailable".to_string();
        }
    }

    pub(super) fn poll_catalog_response(&mut self) {
        while let Ok(response) = self.shell.worker_bus.catalog.rx.try_recv() {
            if self.shell.worker_bus.catalog.pending_request_id != Some(response.request_id) {
                continue;
            }
            self.shell.worker_bus.catalog.clear_request();
            match response.result {
                Ok(catalog) => {
                    self.shell.features.presets.catalog = catalog;
                    if let Some(name) = self.shell.features.presets.pending_selection.take() {
                        self.shell.features.presets.selected_name = Some(name);
                    } else if self
                        .shell
                        .features
                        .presets
                        .selected_name
                        .as_deref()
                        .is_some_and(|name| {
                            self.shell.features.presets.catalog.preset(name).is_none()
                        })
                    {
                        self.shell.features.presets.selected_name = None;
                    }
                    self.shell.features.presets.error.clear();
                    self.set_notice("Search catalog updated");
                }
                Err(error) => {
                    self.shell.features.presets.pending_selection = None;
                    self.shell.features.presets.error = error;
                }
            }
        }
    }

    pub(super) fn apply_selected_preset(&mut self) {
        let Some(name) = self.shell.features.presets.selected_name.clone() else {
            return;
        };
        let Some(preset) = self.shell.features.presets.catalog.preset(&name).cloned() else {
            self.shell.features.presets.error = format!("Preset not found: {name}");
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

        self.shell.runtime.query_state.query = preset.query;
        self.shell.runtime.use_filelist = use_filelist;
        self.shell.runtime.use_regex = preset.regex;
        self.shell.runtime.ignore_case = preset.ignore_case;
        self.shell.runtime.include_files = include_files;
        self.shell.runtime.include_dirs = include_dirs;
        self.shell.ui.ignore_list_enabled = preset.ignore_enabled;
        self.shell.features.presets.error.clear();
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
        self.set_notice(format!("Applied preset: {name}"));
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

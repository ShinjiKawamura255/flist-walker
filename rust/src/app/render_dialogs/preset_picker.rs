use crate::app::{
    render::{EmacsSinglelineOptions, RenderPresetPickerCommand},
    render_panels, FlistWalkerApp,
};
use crate::search_catalog::{PresetEntryType, PresetSortMode, PresetSource};
use crate::ui_model::normalize_path_for_display;
use eframe::egui;

pub(in crate::app) const MANAGE_NAMED_ROOTS_LABEL: &str = "Manage named roots...";
pub(in crate::app) const PRESET_PICKER_FOOTER_HINT: &str =
    "Type to filter · Up/Down to select · Enter to apply · F2 to edit · Esc to close";

pub(in crate::app) fn preset_picker_modal_width(available_width: f32) -> f32 {
    (available_width - 24.0).clamp(600.0, 720.0)
}

pub(in crate::app) fn preset_summary(app: &FlistWalkerApp, catalog_index: usize) -> Option<String> {
    let preset = app
        .shell
        .features
        .presets
        .catalog
        .presets
        .get(catalog_index)?;
    let root = app
        .shell
        .features
        .presets
        .catalog
        .resolve_preset_root(preset);
    let query = if preset.query.is_empty() {
        "(empty query)"
    } else {
        &preset.query
    };
    let depth = preset.max_depth.value().map_or_else(
        || "Depth: All".to_string(),
        |depth| format!("Depth: ≤ {depth}"),
    );
    Some(format!(
        "{}  —  {}  —  {}",
        normalize_path_for_display(&root),
        query,
        depth
    ))
}

pub(in crate::app) fn entry_type_label(value: PresetEntryType) -> &'static str {
    match value {
        PresetEntryType::All => "Files and folders",
        PresetEntryType::File => "Files",
        PresetEntryType::Folder => "Folders",
    }
}

pub(in crate::app) fn source_label(value: PresetSource) -> &'static str {
    match value {
        PresetSource::Auto => "Auto",
        PresetSource::Filelist => "FileList",
        PresetSource::Walker => "Walker",
    }
}

pub(in crate::app) fn sort_label(value: PresetSortMode) -> &'static str {
    match value {
        PresetSortMode::Score => "Score",
        PresetSortMode::NameAsc => "Name ascending",
        PresetSortMode::NameDesc => "Name descending",
        PresetSortMode::ModifiedDesc => "Modified newest",
        PresetSortMode::ModifiedAsc => "Modified oldest",
        PresetSortMode::CreatedDesc => "Created newest",
        PresetSortMode::CreatedAsc => "Created oldest",
        PresetSortMode::SizeDesc => "Size largest",
        PresetSortMode::SizeAsc => "Size smallest",
    }
}

fn render_picker(app: &mut FlistWalkerApp, ui: &mut egui::Ui) {
    ui.horizontal(|ui| {
        ui.heading("Presets");
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if ui
                .add_enabled(
                    !app.shell.worker_bus.catalog.in_progress,
                    egui::Button::new(MANAGE_NAMED_ROOTS_LABEL),
                )
                .clicked()
            {
                app.queue_render_command(crate::app::render::RenderCommand::PresetPicker(
                    RenderPresetPickerCommand::OpenNamedRoots,
                ));
            }
        });
    });
    ui.label("Search saved pure-search presets. Applying one never opens or executes a result.");
    ui.add_space(6.0);

    let previous_query = app.shell.features.presets.picker.query.clone();
    let output = FlistWalkerApp::emacs_singleline_text_edit(
        ui,
        &mut app.shell.features.presets.picker.query,
        &mut app.shell.runtime.query_state.kill_buffer,
        app.shell.runtime.emacs_keybindings_enabled,
        app.shell.ui.ime_composition_active,
        EmacsSinglelineOptions::new(
            Some(egui::Id::new(FlistWalkerApp::PRESET_PICKER_QUERY_ID)),
            f32::INFINITY,
            Some("Filter preset names..."),
        ),
    );
    let emacs_text_changed = app.shell.features.presets.picker.query != previous_query;
    let response = output.response;
    if app.shell.features.presets.picker.focus_requested {
        response.request_focus();
        app.shell.features.presets.picker.focus_requested = false;
    }
    if response.changed() || emacs_text_changed {
        app.refresh_preset_picker_matches();
    }

    ui.add_space(6.0);
    if app.shell.worker_bus.catalog.in_progress {
        ui.label("Loading presets...");
    } else if !app.shell.features.presets.picker.error.is_empty() {
        ui.colored_label(
            ui.visuals().error_fg_color,
            format!("Error: {}", app.shell.features.presets.picker.error),
        );
    } else {
        let matched = app
            .shell
            .features
            .presets
            .picker
            .matched_catalog_indices
            .clone();
        let selected = app.shell.features.presets.picker.selected_match;
        let follow_selection = render_panels::selection_scroll_requested(
            ui,
            "preset-picker-results",
            egui::Id::new((selected, &app.shell.features.presets.picker.query)),
        );
        #[cfg(test)]
        let mut selected_rect = None;
        let scroll = egui::ScrollArea::vertical()
            .id_salt("preset-picker-results")
            .animated(false)
            .max_height(320.0)
            .auto_shrink([false, false])
            .show(ui, |ui| {
                if matched.is_empty() {
                    ui.label(if app.shell.features.presets.catalog.presets.is_empty() {
                        "No presets are configured"
                    } else {
                        "No matching presets"
                    });
                }
                for (match_index, catalog_index) in matched.iter().copied().enumerate() {
                    let Some(preset) = app
                        .shell
                        .features
                        .presets
                        .catalog
                        .presets
                        .get(catalog_index)
                    else {
                        continue;
                    };
                    let name = preset.name.clone();
                    let summary = preset_summary(app, catalog_index).unwrap_or_default();
                    let row = ui.vertical(|ui| {
                        let response = FlistWalkerApp::selectable_row(
                            ui,
                            selected == Some(match_index),
                            &name,
                        );
                        ui.weak(summary);
                        response
                    });
                    if selected == Some(match_index) && follow_selection {
                        row.response.scroll_to_me(None);
                    }
                    #[cfg(test)]
                    if selected == Some(match_index) {
                        selected_rect = Some(row.response.rect);
                    }
                    if row.inner.double_clicked() {
                        app.select_preset_picker_match(match_index);
                        app.queue_render_command(crate::app::render::RenderCommand::PresetPicker(
                            RenderPresetPickerCommand::Apply,
                        ));
                    } else if row.inner.clicked() {
                        app.select_preset_picker_match(match_index);
                    }
                    ui.add_space(3.0);
                }
            });
        #[cfg(test)]
        render_panels::record_list_scroll(
            ui.ctx(),
            "preset-picker-results",
            scroll.id,
            scroll.state.offset,
            scroll.inner_rect,
            selected_rect,
        );
        #[cfg(not(test))]
        let _ = scroll;
    }

    ui.add_space(6.0);
    ui.label(PRESET_PICKER_FOOTER_HINT);
    ui.add_space(4.0);
    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
        if ui.button("Close").clicked() {
            app.queue_render_command(crate::app::render::RenderCommand::PresetPicker(
                RenderPresetPickerCommand::Close,
            ));
        }
        let can_edit = !app.shell.worker_bus.catalog.in_progress
            && app.shell.features.presets.picker.selected_match.is_some();
        if ui
            .add_enabled(can_edit, egui::Button::new("Edit"))
            .clicked()
        {
            app.queue_render_command(crate::app::render::RenderCommand::PresetPicker(
                RenderPresetPickerCommand::StartEdit,
            ));
        }
        if ui
            .add_enabled(can_edit, egui::Button::new("Delete"))
            .clicked()
        {
            app.queue_render_command(crate::app::render::RenderCommand::PresetPicker(
                RenderPresetPickerCommand::StartDelete,
            ));
        }
        if ui
            .add_enabled(
                !app.shell.worker_bus.catalog.in_progress,
                egui::Button::new("Add"),
            )
            .clicked()
        {
            app.queue_render_command(crate::app::render::RenderCommand::PresetPicker(
                RenderPresetPickerCommand::Add,
            ));
        }
    });
}

fn render_editor(app: &mut FlistWalkerApp, ui: &mut egui::Ui) {
    let primary = FlistWalkerApp::primary_shortcut_label();
    let is_new = app
        .shell
        .features
        .presets
        .picker
        .editor
        .original_name
        .is_empty();
    ui.heading(if is_new { "Add preset" } else { "Edit preset" });
    ui.label(if is_new {
        "Save the current pure-search state as a preset without applying or executing anything."
    } else {
        "Changes are saved to the preset catalog and are not applied to the current tab."
    });
    ui.add_space(6.0);

    let named_roots = app
        .shell
        .features
        .presets
        .catalog
        .named_roots
        .iter()
        .map(|root| root.name.clone())
        .collect::<Vec<_>>();
    let mut manage_named_roots = false;
    let mut browse_root = false;
    let busy = app.shell.worker_bus.catalog.in_progress;
    let emacs_enabled = app.shell.runtime.emacs_keybindings_enabled;
    let ime_composition_active = app.shell.ui.ime_composition_active;
    let shell = &mut app.shell;
    let kill_buffer = &mut shell.runtime.query_state.kill_buffer;
    let editor = &mut shell.features.presets.picker.editor;
    egui::Grid::new("preset-editor-fields")
        .num_columns(2)
        .spacing([12.0, 7.0])
        .show(ui, |ui| {
            ui.label("Name");
            let name_output = FlistWalkerApp::emacs_singleline_text_edit(
                ui,
                &mut editor.name,
                kill_buffer,
                emacs_enabled,
                ime_composition_active,
                EmacsSinglelineOptions::new(Some(egui::Id::new("preset-editor-name")), 430.0, None),
            );
            let name_response = name_output.response;
            if editor.focus_requested {
                name_response.request_focus();
                editor.focus_requested = false;
            }
            ui.end_row();

            ui.label("Named root");
            let root_label = editor
                .root_name
                .clone()
                .unwrap_or_else(|| "Path snapshot".to_string());
            ui.horizontal(|ui| {
                egui::ComboBox::from_id_salt("preset-editor-named-root")
                    .selected_text(&root_label)
                    .show_ui(ui, |ui| {
                        ui.selectable_value(&mut editor.root_name, None, "Path snapshot");
                        for name in &named_roots {
                            ui.selectable_value(&mut editor.root_name, Some(name.clone()), name);
                        }
                    });
                if ui.button("Manage...").clicked() {
                    manage_named_roots = true;
                }
            });
            ui.end_row();

            ui.label(if editor.root_name.is_some() {
                "Fallback root"
            } else {
                "Root"
            });
            ui.horizontal(|ui| {
                FlistWalkerApp::emacs_singleline_text_edit(
                    ui,
                    &mut editor.root_path,
                    kill_buffer,
                    emacs_enabled,
                    ime_composition_active,
                    EmacsSinglelineOptions::new(
                        Some(egui::Id::new("preset-editor-root-path")),
                        345.0,
                        Some("Absolute path"),
                    ),
                );
                if ui
                    .add_enabled(!busy, egui::Button::new("Browse..."))
                    .clicked()
                {
                    browse_root = true;
                }
            });
            ui.end_row();

            ui.label("Query");
            FlistWalkerApp::emacs_singleline_text_edit(
                ui,
                &mut editor.query,
                kill_buffer,
                emacs_enabled,
                ime_composition_active,
                EmacsSinglelineOptions::new(
                    Some(egui::Id::new("preset-editor-query")),
                    430.0,
                    Some("Empty query is allowed"),
                ),
            );
            ui.end_row();

            ui.label("Entry type");
            egui::ComboBox::from_id_salt("preset-editor-entry-type")
                .selected_text(entry_type_label(editor.entry_type))
                .show_ui(ui, |ui| {
                    for value in [
                        PresetEntryType::All,
                        PresetEntryType::File,
                        PresetEntryType::Folder,
                    ] {
                        ui.selectable_value(&mut editor.entry_type, value, entry_type_label(value));
                    }
                });
            ui.end_row();

            ui.label("Source");
            egui::ComboBox::from_id_salt("preset-editor-source")
                .selected_text(source_label(editor.source))
                .show_ui(ui, |ui| {
                    for value in [
                        PresetSource::Auto,
                        PresetSource::Filelist,
                        PresetSource::Walker,
                    ] {
                        ui.selectable_value(&mut editor.source, value, source_label(value));
                    }
                });
            ui.end_row();

            ui.label("Sort");
            egui::ComboBox::from_id_salt("preset-editor-sort")
                .selected_text(sort_label(editor.sort))
                .show_ui(ui, |ui| {
                    for value in [
                        PresetSortMode::Score,
                        PresetSortMode::NameAsc,
                        PresetSortMode::NameDesc,
                        PresetSortMode::ModifiedDesc,
                        PresetSortMode::ModifiedAsc,
                        PresetSortMode::CreatedDesc,
                        PresetSortMode::CreatedAsc,
                        PresetSortMode::SizeDesc,
                        PresetSortMode::SizeAsc,
                    ] {
                        ui.selectable_value(&mut editor.sort, value, sort_label(value));
                    }
                });
            ui.end_row();

            ui.label("Max depth");
            ui.horizontal(|ui| {
                let mut unlimited = editor.max_depth.is_unlimited();
                if render_panels::centered_checkbox(ui, &mut unlimited, "Unlimited").changed() {
                    editor.max_depth = if unlimited {
                        crate::indexer::MaxDepth::unlimited()
                    } else {
                        crate::indexer::MaxDepth::limited(1).expect("one is a valid depth")
                    };
                }
                if !unlimited {
                    let mut depth = editor.max_depth.value().unwrap_or(1);
                    if ui
                        .add(egui::DragValue::new(&mut depth).range(1..=u32::MAX as usize))
                        .changed()
                    {
                        editor.max_depth = crate::indexer::MaxDepth::limited(depth)
                            .expect("drag value is clamped to a positive depth");
                    }
                }
            });
            ui.end_row();
        });

    ui.add_space(6.0);
    ui.horizontal_wrapped(|ui| {
        ui.checkbox(&mut editor.follow_links, "Follow links");
        ui.checkbox(&mut editor.regex, "Regex");
        ui.checkbox(&mut editor.ignore_case, "Ignore case");
        ui.checkbox(&mut editor.ignore_enabled, "Use ignore list");
    });
    if !editor.error.is_empty() {
        ui.add_space(6.0);
        ui.colored_label(
            ui.visuals().error_fg_color,
            format!("Error: {}", editor.error),
        );
    }

    ui.add_space(8.0);
    ui.horizontal(|ui| {
        ui.label(if busy {
            "Saving preset...".to_string()
        } else {
            format!("{primary}+Enter to save · Esc to discard draft")
        });
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if ui.add_enabled(!busy, egui::Button::new("Save")).clicked() {
                app.queue_render_command(crate::app::render::RenderCommand::PresetPicker(
                    RenderPresetPickerCommand::SaveEdit,
                ));
            }
            if ui.add_enabled(!busy, egui::Button::new("Cancel")).clicked() {
                app.queue_render_command(crate::app::render::RenderCommand::PresetPicker(
                    RenderPresetPickerCommand::CancelEdit,
                ));
            }
        });
    });
    if manage_named_roots {
        app.queue_render_command(crate::app::render::RenderCommand::PresetPicker(
            RenderPresetPickerCommand::OpenNamedRoots,
        ));
    }
    if browse_root {
        app.queue_render_command(crate::app::render::RenderCommand::PresetPicker(
            RenderPresetPickerCommand::BrowsePresetRoot,
        ));
    }
}

fn render_preset_delete_confirmation(app: &mut FlistWalkerApp, ui: &mut egui::Ui) {
    let preset = app
        .shell
        .features
        .presets
        .picker
        .selected_match
        .and_then(|match_index| {
            app.shell
                .features
                .presets
                .picker
                .matched_catalog_indices
                .get(match_index)
        })
        .and_then(|catalog_index| {
            app.shell
                .features
                .presets
                .catalog
                .presets
                .get(*catalog_index)
        });
    let name = preset
        .map(|preset| preset.name.as_str())
        .unwrap_or("(missing)");

    ui.heading("Delete preset?");
    ui.label(name);
    ui.add_space(6.0);
    ui.label(
        "This removes only the saved preset. The current tab and search results are unchanged.",
    );
    let picker = &app.shell.features.presets.picker;
    if !picker.error.is_empty() {
        ui.add_space(6.0);
        ui.colored_label(
            ui.visuals().error_fg_color,
            format!("Error: {}", picker.error),
        );
    }
    ui.add_space(8.0);
    let busy = app.shell.worker_bus.catalog.in_progress;
    ui.horizontal(|ui| {
        ui.label(if busy {
            "Deleting preset..."
        } else {
            "Esc to cancel"
        });
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if ui
                .add_enabled(!busy, egui::Button::new("Delete preset"))
                .clicked()
            {
                app.queue_render_command(crate::app::render::RenderCommand::PresetPicker(
                    RenderPresetPickerCommand::ConfirmDelete,
                ));
            }
            if ui.add_enabled(!busy, egui::Button::new("Cancel")).clicked() {
                app.queue_render_command(crate::app::render::RenderCommand::PresetPicker(
                    RenderPresetPickerCommand::CancelDelete,
                ));
            }
        });
    });
}

fn render_named_root_editor(app: &mut FlistWalkerApp, ui: &mut egui::Ui) {
    let primary = FlistWalkerApp::primary_shortcut_label();
    let is_new = app
        .shell
        .features
        .presets
        .picker
        .named_roots
        .editor
        .original_name
        .is_none();
    ui.heading(if is_new {
        "Add named root"
    } else {
        "Edit named root"
    });
    ui.label("A named root gives one or more presets a reusable search location.");
    ui.add_space(6.0);

    let current_root = normalize_path_for_display(&app.shell.runtime.root);
    let busy = app.shell.worker_bus.catalog.in_progress;
    let mut browse_path = false;
    let emacs_enabled = app.shell.runtime.emacs_keybindings_enabled;
    let ime_composition_active = app.shell.ui.ime_composition_active;
    let shell = &mut app.shell;
    let kill_buffer = &mut shell.runtime.query_state.kill_buffer;
    let editor = &mut shell.features.presets.picker.named_roots.editor;
    egui::Grid::new("named-root-editor-fields")
        .num_columns(2)
        .spacing([12.0, 7.0])
        .show(ui, |ui| {
            ui.label("Name");
            let name_output = FlistWalkerApp::emacs_singleline_text_edit(
                ui,
                &mut editor.name,
                kill_buffer,
                emacs_enabled,
                ime_composition_active,
                EmacsSinglelineOptions::new(
                    Some(egui::Id::new("named-root-editor-name")),
                    430.0,
                    None,
                ),
            );
            let response = name_output.response;
            if editor.focus_requested {
                response.request_focus();
                editor.focus_requested = false;
            }
            ui.end_row();

            ui.label("Path");
            ui.horizontal(|ui| {
                FlistWalkerApp::emacs_singleline_text_edit(
                    ui,
                    &mut editor.path,
                    kill_buffer,
                    emacs_enabled,
                    ime_composition_active,
                    EmacsSinglelineOptions::new(
                        Some(egui::Id::new("named-root-editor-path")),
                        245.0,
                        Some("Absolute path"),
                    ),
                );
                if ui
                    .add_enabled(!busy, egui::Button::new("Browse..."))
                    .clicked()
                {
                    browse_path = true;
                }
                if ui.button("Use current root").clicked() {
                    editor.path.clone_from(&current_root);
                }
            });
            ui.end_row();
        });

    if !editor.error.is_empty() {
        ui.add_space(6.0);
        ui.colored_label(
            ui.visuals().error_fg_color,
            format!("Error: {}", editor.error),
        );
    }
    ui.add_space(8.0);
    ui.horizontal(|ui| {
        ui.label(if busy {
            "Saving named root...".to_string()
        } else {
            format!("{primary}+Enter to save · Esc to cancel")
        });
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if ui.add_enabled(!busy, egui::Button::new("Save")).clicked() {
                app.queue_render_command(crate::app::render::RenderCommand::PresetPicker(
                    RenderPresetPickerCommand::SaveNamedRoot,
                ));
            }
            if ui.add_enabled(!busy, egui::Button::new("Cancel")).clicked() {
                app.queue_render_command(crate::app::render::RenderCommand::PresetPicker(
                    RenderPresetPickerCommand::CancelNamedRootEdit,
                ));
            }
        });
    });
    if browse_path {
        app.queue_render_command(crate::app::render::RenderCommand::PresetPicker(
            RenderPresetPickerCommand::BrowseNamedRoot,
        ));
    }
}

fn render_named_root_delete_confirmation(app: &mut FlistWalkerApp, ui: &mut egui::Ui) {
    let manager = &app.shell.features.presets.picker.named_roots;
    let root = manager
        .selected_index
        .and_then(|index| app.shell.features.presets.catalog.named_roots.get(index));
    let name = root.map(|root| root.name.as_str()).unwrap_or("(missing)");
    let path = root
        .map(|root| normalize_path_for_display(&root.path))
        .unwrap_or_default();
    let linked_count = app
        .shell
        .features
        .presets
        .catalog
        .presets
        .iter()
        .filter(|preset| {
            preset
                .root_name
                .as_deref()
                .is_some_and(|candidate| candidate.eq_ignore_ascii_case(name))
        })
        .count();

    ui.heading("Delete named root?");
    ui.label(format!("{name}  —  {path}"));
    ui.add_space(6.0);
    ui.label(if linked_count == 0 {
        "No presets currently reference this named root.".to_string()
    } else {
        format!("{linked_count} preset(s) will keep working with their saved path snapshots.")
    });
    if !manager.error.is_empty() {
        ui.add_space(6.0);
        ui.colored_label(
            ui.visuals().error_fg_color,
            format!("Error: {}", manager.error),
        );
    }
    ui.add_space(8.0);
    let busy = app.shell.worker_bus.catalog.in_progress;
    ui.horizontal(|ui| {
        ui.label(if busy {
            "Deleting named root..."
        } else {
            "This removes only the named root entry."
        });
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if ui
                .add_enabled(!busy, egui::Button::new("Delete root"))
                .clicked()
            {
                app.queue_render_command(crate::app::render::RenderCommand::PresetPicker(
                    RenderPresetPickerCommand::ConfirmDeleteNamedRoot,
                ));
            }
            if ui.add_enabled(!busy, egui::Button::new("Cancel")).clicked() {
                app.queue_render_command(crate::app::render::RenderCommand::PresetPicker(
                    RenderPresetPickerCommand::CancelDeleteNamedRoot,
                ));
            }
        });
    });
}

fn render_named_root_list(app: &mut FlistWalkerApp, ui: &mut egui::Ui) {
    ui.heading("Named roots");
    ui.label("Manage reusable search locations for presets.");
    ui.add_space(6.0);

    let roots = app
        .shell
        .features
        .presets
        .catalog
        .named_roots
        .iter()
        .map(|root| (root.name.clone(), normalize_path_for_display(&root.path)))
        .collect::<Vec<_>>();
    let selected = app.shell.features.presets.picker.named_roots.selected_index;
    let follow_selection = render_panels::selection_scroll_requested(
        ui,
        "named-root-results",
        egui::Id::new(selected),
    );
    #[cfg(test)]
    let mut selected_rect = None;
    let scroll = egui::ScrollArea::vertical()
        .id_salt("named-root-results")
        .animated(false)
        .max_height(320.0)
        .auto_shrink([false, false])
        .show(ui, |ui| {
            if roots.is_empty() {
                ui.label("No named roots are configured");
            }
            for (index, (name, path)) in roots.iter().enumerate() {
                let row = ui.vertical(|ui| {
                    let response =
                        FlistWalkerApp::selectable_row(ui, selected == Some(index), name);
                    ui.weak(path);
                    response
                });
                if selected == Some(index) && follow_selection {
                    row.response.scroll_to_me(None);
                }
                #[cfg(test)]
                if selected == Some(index) {
                    selected_rect = Some(row.response.rect);
                }
                if row.inner.double_clicked() {
                    app.shell.features.presets.picker.named_roots.selected_index = Some(index);
                    app.queue_render_command(crate::app::render::RenderCommand::PresetPicker(
                        RenderPresetPickerCommand::EditNamedRoot,
                    ));
                } else if row.inner.clicked() {
                    let manager = &mut app.shell.features.presets.picker.named_roots;
                    manager.selected_index = Some(index);
                    manager.confirm_delete = false;
                    manager.error.clear();
                }
                ui.add_space(3.0);
            }
        });
    #[cfg(test)]
    render_panels::record_list_scroll(
        ui.ctx(),
        "named-root-results",
        scroll.id,
        scroll.state.offset,
        scroll.inner_rect,
        selected_rect,
    );
    #[cfg(not(test))]
    let _ = scroll;

    let manager = &app.shell.features.presets.picker.named_roots;
    if !manager.error.is_empty() {
        ui.add_space(6.0);
        ui.colored_label(
            ui.visuals().error_fg_color,
            format!("Error: {}", manager.error),
        );
    }
    ui.add_space(6.0);
    let busy = app.shell.worker_bus.catalog.in_progress;
    let has_selection = manager.selected_index.is_some();
    ui.horizontal(|ui| {
        ui.label("Up/Down to select · F2 to edit · Delete to remove · Esc to go back");
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if ui.add_enabled(!busy, egui::Button::new("Back")).clicked() {
                app.queue_render_command(crate::app::render::RenderCommand::PresetPicker(
                    RenderPresetPickerCommand::CloseNamedRoots,
                ));
            }
            if ui
                .add_enabled(!busy && has_selection, egui::Button::new("Delete"))
                .clicked()
            {
                app.queue_render_command(crate::app::render::RenderCommand::PresetPicker(
                    RenderPresetPickerCommand::StartDeleteNamedRoot,
                ));
            }
            if ui
                .add_enabled(!busy && has_selection, egui::Button::new("Edit"))
                .clicked()
            {
                app.queue_render_command(crate::app::render::RenderCommand::PresetPicker(
                    RenderPresetPickerCommand::EditNamedRoot,
                ));
            }
            if ui.add_enabled(!busy, egui::Button::new("Add")).clicked() {
                app.queue_render_command(crate::app::render::RenderCommand::PresetPicker(
                    RenderPresetPickerCommand::AddNamedRoot,
                ));
            }
        });
    });
}

fn render_named_root_manager(app: &mut FlistWalkerApp, ui: &mut egui::Ui) {
    if app.shell.features.presets.picker.named_roots.editor.open {
        render_named_root_editor(app, ui);
    } else if app.shell.features.presets.picker.named_roots.confirm_delete {
        render_named_root_delete_confirmation(app, ui);
    } else {
        render_named_root_list(app, ui);
    }
}

pub(super) fn render(app: &mut FlistWalkerApp, ctx: &egui::Context) {
    if !app.shell.features.presets.picker.open {
        return;
    }

    let available_width = ctx.input(|input| input.content_rect().width());
    egui::Modal::new(egui::Id::new("preset-picker-modal")).show(ctx, |ui| {
        ui.set_min_width(preset_picker_modal_width(available_width));
        if app.shell.features.presets.picker.named_roots.open {
            render_named_root_manager(app, ui);
        } else if app.shell.features.presets.picker.editor.open {
            render_editor(app, ui);
        } else if app.shell.features.presets.picker.confirm_delete {
            render_preset_delete_confirmation(app, ui);
        } else {
            render_picker(app, ui);
        }
    });
}

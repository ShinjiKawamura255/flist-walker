use crate::app::{render::RenderPresetPickerCommand, FlistWalkerApp};
use crate::search_catalog::{PresetEntryType, PresetSortMode, PresetSource};
use eframe::egui;

fn preset_summary(app: &FlistWalkerApp, catalog_index: usize) -> Option<String> {
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
    Some(format!("{}  —  {}", root.display(), query))
}

fn entry_type_label(value: PresetEntryType) -> &'static str {
    match value {
        PresetEntryType::All => "Files and folders",
        PresetEntryType::File => "Files",
        PresetEntryType::Folder => "Folders",
    }
}

fn source_label(value: PresetSource) -> &'static str {
    match value {
        PresetSource::Auto => "Auto",
        PresetSource::Filelist => "FileList",
        PresetSource::Walker => "Walker",
    }
}

fn sort_label(value: PresetSortMode) -> &'static str {
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
    ui.heading("Presets");
    ui.label("Search saved pure-search presets. Applying one never opens or executes a result.");
    ui.add_space(6.0);

    let response = ui.add(
        egui::TextEdit::singleline(&mut app.shell.features.presets.picker.query)
            .id(egui::Id::new(FlistWalkerApp::PRESET_PICKER_QUERY_ID))
            .desired_width(f32::INFINITY)
            .hint_text("Filter preset names..."),
    );
    if app.shell.features.presets.picker.focus_requested {
        response.request_focus();
        app.shell.features.presets.picker.focus_requested = false;
    }
    if response.changed() {
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
        egui::ScrollArea::vertical()
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
    }

    ui.add_space(6.0);
    ui.horizontal(|ui| {
        ui.label("Type to filter · Up/Down to select · Enter to apply · F2 to edit · Esc to close");
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
        });
    });
}

fn render_editor(app: &mut FlistWalkerApp, ui: &mut egui::Ui) {
    let primary = FlistWalkerApp::primary_shortcut_label();
    ui.heading("Edit preset");
    ui.label("Changes are saved to the preset catalog and are not applied to the current tab.");
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
    let editor = &mut app.shell.features.presets.picker.editor;
    egui::Grid::new("preset-editor-fields")
        .num_columns(2)
        .spacing([12.0, 7.0])
        .show(ui, |ui| {
            ui.label("Name");
            let name_response = ui.add(
                egui::TextEdit::singleline(&mut editor.name)
                    .id(egui::Id::new("preset-editor-name"))
                    .desired_width(430.0),
            );
            if editor.focus_requested {
                name_response.request_focus();
                editor.focus_requested = false;
            }
            ui.end_row();

            ui.label("Named root");
            let root_label = editor.root_name.as_deref().unwrap_or("Path snapshot");
            egui::ComboBox::from_id_salt("preset-editor-named-root")
                .selected_text(root_label)
                .show_ui(ui, |ui| {
                    ui.selectable_value(&mut editor.root_name, None, "Path snapshot");
                    for name in &named_roots {
                        ui.selectable_value(&mut editor.root_name, Some(name.clone()), name);
                    }
                });
            ui.end_row();

            ui.label(if editor.root_name.is_some() {
                "Fallback root"
            } else {
                "Root"
            });
            ui.add(
                egui::TextEdit::singleline(&mut editor.root_path)
                    .desired_width(430.0)
                    .hint_text("Absolute path"),
            );
            ui.end_row();

            ui.label("Query");
            ui.add(
                egui::TextEdit::singleline(&mut editor.query)
                    .desired_width(430.0)
                    .hint_text("Empty query is allowed"),
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
        });

    ui.add_space(6.0);
    ui.horizontal_wrapped(|ui| {
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
    let busy = app.shell.worker_bus.catalog.in_progress;
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
}

pub(super) fn render(app: &mut FlistWalkerApp, ctx: &egui::Context) {
    if !app.shell.features.presets.picker.open {
        return;
    }

    egui::Modal::new(egui::Id::new("preset-picker-modal")).show(ctx, |ui| {
        ui.set_min_width(620.0);
        if app.shell.features.presets.picker.editor.open {
            render_editor(app, ui);
        } else {
            render_picker(app, ui);
        }
    });
}

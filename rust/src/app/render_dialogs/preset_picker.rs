use crate::app::{render::RenderPresetPickerCommand, FlistWalkerApp};
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

pub(super) fn render(app: &mut FlistWalkerApp, ctx: &egui::Context) {
    if !app.shell.features.presets.picker.open {
        return;
    }

    let mut apply = false;
    let mut close = false;
    egui::Modal::new(egui::Id::new("preset-picker-modal")).show(ctx, |ui| {
        ui.set_min_width(620.0);
        ui.heading("Presets");
        ui.label(
            "Search saved pure-search presets. Applying one never opens or executes a result.",
        );
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
                            apply = true;
                        } else if row.inner.clicked() {
                            app.select_preset_picker_match(match_index);
                        }
                        ui.add_space(3.0);
                    }
                });
        }

        ui.add_space(6.0);
        ui.horizontal(|ui| {
            ui.label("Type to filter · Up/Down to select · Enter to apply · Esc to close");
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui.button("Close").clicked() {
                    close = true;
                }
            });
        });
    });

    if apply {
        app.queue_render_command(crate::app::render::RenderCommand::PresetPicker(
            RenderPresetPickerCommand::Apply,
        ));
    }
    if close {
        app.queue_render_command(crate::app::render::RenderCommand::PresetPicker(
            RenderPresetPickerCommand::Close,
        ));
    }
}

use crate::app::render::RenderCommand;
use crate::app::settings_dialog::SettingsView;
use crate::app::FlistWalkerApp;
use crate::runtime_config::EditableSettings;
use eframe::egui;

#[derive(Clone, Copy)]
enum Action {
    Save,
    Reload,
    OpenJson,
    Close,
    Retry,
}

pub(super) fn render(app: &mut FlistWalkerApp, ctx: &egui::Context) {
    if !app.settings_dialog.is_open() {
        return;
    }
    let can_open_json = !app.shell.worker_bus.config_open.in_progress();
    let mut action = None;
    egui::Modal::new(egui::Id::new("gui-settings-modal")).show(ctx, |ui| {
        ui.set_min_width(540.0);
        ui.heading("Settings");
        ui.label("Saved changes take effect the next time FlistWalker starts.");
        ui.add_space(8.0);
        match &mut app.settings_dialog.view {
            SettingsView::Closed => {}
            SettingsView::Loading => {
                ui.spinner();
                ui.label("Loading settings...");
                if ui.button("Cancel").clicked() {
                    action = Some(Action::Close);
                }
            }
            SettingsView::Reloading { .. } => {
                ui.spinner();
                ui.label("Reloading settings...");
                if ui.button("Close settings and discard draft").clicked() {
                    action = Some(Action::Close);
                }
            }
            SettingsView::Saving { .. } => {
                ui.spinner();
                ui.label("Saving settings...");
            }
            SettingsView::Failed(error) => {
                ui.colored_label(ui.visuals().error_fg_color, error);
                if ui.button("Retry").clicked() {
                    action = Some(Action::Retry);
                }
            }
            SettingsView::Editing {
                baseline,
                draft,
                limit_text,
                error,
                confirm_reload,
            } => {
                egui::ScrollArea::vertical().max_height(430.0).show(ui, |ui| {
                    ui.heading("Startup and history");
                    ui.checkbox(&mut draft.restore_tabs_enabled, "Restore previous tabs");
                    ui.label("Explicit startup options take priority over restored tabs.");
                    let mut persist_history = !draft.history_persist_disabled;
                    if ui
                        .checkbox(&mut persist_history, "Save search history")
                        .changed()
                    {
                        draft.history_persist_disabled = !persist_history;
                    }
                    ui.label("Turning this off does not delete existing history. Restored tab queries are stored separately.");
                    ui.add_space(10.0);
                    ui.heading("Keyboard");
                    ui.checkbox(&mut draft.emacs_keybindings_enabled, "Use Emacs-style shortcuts");
                    ui.add_enabled_ui(draft.emacs_keybindings_enabled, |ui| {
                        ui.checkbox(
                            &mut draft.ctrl_w_deletes_word_in_query,
                            "Ctrl+W deletes a word in the search box",
                        );
                    });
                    ui.label("When disabled, the Ctrl+W preference is kept for later. Tab close shortcuts remain available outside search editing.");
                    ui.checkbox(
                        &mut draft.tab_pin_moves_to_next_row,
                        "Move to the next row after pinning with Tab",
                    );
                    ui.add_space(10.0);
                    ui.heading("Search");
                    ui.horizontal(|ui| {
                        ui.label("Walker entry limit");
                        ui.add(egui::TextEdit::singleline(limit_text).desired_width(120.0));
                    });
                    ui.label("Maximum candidates collected by Walker, not the number of results shown.");
                    if let Some(message) = error {
                        ui.colored_label(ui.visuals().error_fg_color, message);
                    }
                });
                ui.add_space(8.0);
                ui.horizontal(|ui| {
                    if ui.button("Save").clicked() {
                        action = Some(Action::Save);
                    }
                    if ui.button("Cancel").clicked() {
                        action = Some(Action::Close);
                    }
                    if ui.button("Reset displayed settings to defaults").clicked() {
                        *draft = EditableSettings::default();
                        *limit_text = draft.walker_max_entries.to_string();
                        *error = None;
                        *confirm_reload = false;
                    }
                });
                let dirty = draft != &baseline.values
                    || limit_text.trim() != baseline.values.walker_max_entries.to_string();
                ui.horizontal(|ui| {
                    let label = if dirty && *confirm_reload {
                        "Discard changes and reload JSON"
                    } else {
                        "Reload JSON"
                    };
                    if ui.button(label).clicked() {
                        action = Some(Action::Reload);
                    }
                    if dirty && *confirm_reload {
                        ui.label("This discards unsaved settings in this dialog.");
                    }
                });
            }
        }
        if !app.settings_dialog.is_busy() {
            ui.add_space(8.0);
            if ui
                .add_enabled(can_open_json, egui::Button::new("Open settings JSON"))
                .clicked()
            {
                action = Some(Action::OpenJson);
            }
            if matches!(app.settings_dialog.view, SettingsView::Failed(_))
                && ui.button("Close").clicked()
            {
                action = Some(Action::Close);
            }
        }
    });
    match action {
        Some(Action::Save) => app.request_settings_save(),
        Some(Action::Reload) => app.request_settings_reload(),
        Some(Action::OpenJson) => app.queue_render_command(RenderCommand::OpenRuntimeConfig),
        Some(Action::Close) => app.close_settings_dialog(),
        Some(Action::Retry) => app.retry_settings_load(),
        None => {}
    }
}

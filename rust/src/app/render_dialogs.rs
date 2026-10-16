pub(super) mod filelist;
pub(super) mod help;
pub(super) mod preset_picker;
pub(in crate::app) mod root_list;
pub(super) mod settings;
pub(super) mod update;

use crate::app::FlistWalkerApp;
use eframe::egui;

pub(super) fn render_filelist_dialogs(app: &mut FlistWalkerApp, ctx: &egui::Context) {
    filelist::render(app, ctx);
}

pub(super) fn render_update_dialog(app: &mut FlistWalkerApp, ctx: &egui::Context) {
    update::render_prompt(app, ctx);
}

pub(super) fn render_update_check_failure_dialog(app: &mut FlistWalkerApp, ctx: &egui::Context) {
    update::render_check_failure(app, ctx);
}

pub(super) fn render_update_install_failure_dialog(app: &mut FlistWalkerApp, ctx: &egui::Context) {
    update::render_install_failure(app, ctx);
}

pub(super) fn render_previous_update_failure_dialog(app: &mut FlistWalkerApp, ctx: &egui::Context) {
    update::render_previous_failure(app, ctx);
}

pub(super) fn render_help_dialog(app: &mut FlistWalkerApp, ctx: &egui::Context) {
    help::render(app, ctx);
}

pub(super) fn render_settings_dialog(app: &mut FlistWalkerApp, ctx: &egui::Context) {
    settings::render(app, ctx);
}

pub(super) fn render_preset_picker_dialog(app: &mut FlistWalkerApp, ctx: &egui::Context) {
    preset_picker::render(app, ctx);
}

pub(super) fn render_manage_root_list_dialog(app: &mut FlistWalkerApp, ctx: &egui::Context) {
    root_list::render(app, ctx);
}

/// Footer actions are anchored to the right. Add the dismiss button first, then
/// affirmative actions, so their visible order is affirmative → dismiss.
pub(in crate::app) fn right_actions<R>(
    ui: &mut egui::Ui,
    actions: impl FnOnce(&mut egui::Ui) -> R,
) -> R {
    // Bound the row height before centering its controls. A bare RTL layout
    // would center them in all remaining space of a resizable Window.
    ui.horizontal(|ui| {
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), actions)
            .inner
    })
    .inner
}

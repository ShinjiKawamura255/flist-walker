pub(super) mod filelist;
mod root_list;
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

pub(super) fn render_manage_root_list_dialog(app: &mut FlistWalkerApp, ctx: &egui::Context) {
    root_list::render(app, ctx);
}

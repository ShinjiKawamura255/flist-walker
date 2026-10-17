use super::EntryDisplayKind;
use eframe::egui;

pub(super) fn selected_fill(dark_mode: bool) -> egui::Color32 {
    if dark_mode {
        egui::Color32::from_rgb(48, 53, 62)
    } else {
        egui::Color32::from_rgb(228, 232, 238)
    }
}

pub(super) fn entry_kind_color(kind: EntryDisplayKind) -> egui::Color32 {
    match kind {
        EntryDisplayKind::Dir => egui::Color32::from_rgb(52, 211, 153),
        EntryDisplayKind::File => egui::Color32::from_rgb(96, 165, 250),
        EntryDisplayKind::Link => egui::Color32::from_rgb(250, 204, 21),
        EntryDisplayKind::Other => egui::Color32::from_rgb(156, 163, 175),
    }
}

pub(super) fn highlight_text_color() -> egui::Color32 {
    egui::Color32::from_rgb(245, 158, 11)
}

impl super::FlistWalkerApp {
    /// Apply the startup theme before the first visible frame.
    pub fn configure_theme(ctx: &egui::Context) {
        apply_theme(ctx, crate::runtime_config::current_runtime_config().theme);
    }
}

pub(super) fn apply_theme(ctx: &egui::Context, theme: crate::runtime_config::ThemeMode) {
    use crate::runtime_config::ThemeMode;
    ctx.set_theme(match theme {
        ThemeMode::System => egui::ThemePreference::System,
        ThemeMode::Dark => egui::ThemePreference::Dark,
        ThemeMode::Light => egui::ThemePreference::Light,
    });
}

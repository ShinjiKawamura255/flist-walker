use super::EntryDisplayKind;
use eframe::egui;

pub(super) fn selected_fill(dark_mode: bool) -> egui::Color32 {
    if dark_mode {
        egui::Color32::from_rgb(48, 53, 62)
    } else {
        egui::Color32::from_rgb(228, 232, 238)
    }
}

pub(super) fn entry_kind_color(kind: EntryDisplayKind, dark_mode: bool) -> egui::Color32 {
    match (kind, dark_mode) {
        (EntryDisplayKind::Dir, true) => egui::Color32::from_rgb(52, 211, 153),
        (EntryDisplayKind::File, true) => egui::Color32::from_rgb(96, 165, 250),
        (EntryDisplayKind::Link, true) => egui::Color32::from_rgb(250, 204, 21),
        (EntryDisplayKind::Other, true) => egui::Color32::from_rgb(156, 163, 175),
        (EntryDisplayKind::Dir, false) => egui::Color32::from_rgb(22, 101, 52),
        (EntryDisplayKind::File, false) => egui::Color32::from_rgb(29, 78, 151),
        (EntryDisplayKind::Link, false) => egui::Color32::from_rgb(133, 77, 14),
        (EntryDisplayKind::Other, false) => egui::Color32::from_rgb(75, 85, 99),
    }
}

pub(super) fn highlight_text_color(dark_mode: bool) -> egui::Color32 {
    if dark_mode {
        egui::Color32::from_rgb(245, 158, 11)
    } else {
        egui::Color32::from_rgb(146, 64, 14)
    }
}

pub(super) fn current_row_marker_color(dark_mode: bool) -> egui::Color32 {
    if dark_mode {
        egui::Color32::LIGHT_BLUE
    } else {
        egui::Color32::from_rgb(29, 78, 151)
    }
}

impl super::FlistWalkerApp {
    /// Apply the startup theme before the first visible frame.
    pub fn configure_theme(ctx: &egui::Context) {
        apply_theme(ctx, crate::runtime_config::current_runtime_config().theme);
    }
}

pub(super) fn apply_theme(ctx: &egui::Context, theme: crate::runtime_config::ThemeMode) {
    use crate::runtime_config::ThemeMode;
    // egui's preference only styles the client area. winit owns native window
    // decorations; release its explicit override when returning to System.
    ctx.send_viewport_cmd_to(
        egui::ViewportId::ROOT,
        egui::ViewportCommand::SetTheme(match theme {
            ThemeMode::System => egui::SystemTheme::SystemDefault,
            ThemeMode::Dark => egui::SystemTheme::Dark,
            ThemeMode::Light => egui::SystemTheme::Light,
        }),
    );
    ctx.set_theme(match theme {
        ThemeMode::System => egui::ThemePreference::System,
        ThemeMode::Dark => egui::ThemePreference::Dark,
        ThemeMode::Light => egui::ThemePreference::Light,
    });
}

pub(super) fn pinned_action_colors(
    dark_mode: bool,
) -> (egui::Color32, egui::Color32, egui::Color32) {
    if dark_mode {
        (
            egui::Color32::from_rgb(77, 61, 37),
            egui::Color32::from_rgb(185, 144, 80),
            egui::Color32::from_rgb(255, 223, 170),
        )
    } else {
        (
            egui::Color32::from_rgb(247, 231, 200),
            egui::Color32::from_rgb(156, 117, 50),
            egui::Color32::from_rgb(100, 66, 20),
        )
    }
}

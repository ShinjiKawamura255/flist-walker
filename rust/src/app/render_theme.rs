use super::EntryDisplayKind;
use eframe::egui;
use egui::Color32;

const LIGHT_SELECTION: Color32 = Color32::from_rgb(195, 207, 224);
const LIGHT_ACCENT: Color32 = Color32::from_rgb(53, 77, 112);
pub(super) const LIGHT_TAB_BAR: Color32 = Color32::from_gray(248);

pub(super) fn default_active_tab_fill(dark_mode: bool) -> Color32 {
    if dark_mode {
        selected_fill(true)
    } else {
        Color32::from_rgb(228, 232, 238)
    }
}

fn light_visuals() -> egui::Visuals {
    let mut visuals = egui::Visuals::light();
    // A half-covered glyph edge gets ~55% opacity without changing glyph metrics.
    visuals.text_options.color_transfer_function =
        egui::epaint::FontColorTransferFunction::Gamma(0.86);
    let panel = Color32::from_gray(232);
    let paper = Color32::from_gray(241);
    let ink = Color32::from_gray(57);
    let strong_ink = Color32::from_gray(44);
    let divider = Color32::from_gray(179);

    visuals.panel_fill = panel;
    visuals.window_fill = Color32::from_gray(237);
    visuals.extreme_bg_color = paper;
    visuals.code_bg_color = Color32::from_gray(223);
    visuals.window_stroke.color = divider;
    visuals.weak_text_color = Some(Color32::from_gray(83));
    visuals.hyperlink_color = LIGHT_ACCENT;
    visuals.warn_fg_color = Color32::from_rgb(128, 71, 22);
    visuals.error_fg_color = Color32::from_rgb(153, 54, 55);
    visuals.selection.bg_fill = LIGHT_SELECTION;
    visuals.selection.stroke.color = LIGHT_ACCENT;
    visuals.text_cursor.stroke.color = LIGHT_ACCENT;

    let widgets = &mut visuals.widgets;
    widgets.noninteractive.bg_fill = panel;
    widgets.noninteractive.weak_bg_fill = panel;
    widgets.noninteractive.bg_stroke.color = divider;
    widgets.noninteractive.fg_stroke.color = ink;
    let idle_fill = Color32::from_gray(201);
    let hover_fill = Color32::from_gray(189);
    for (state, fill, border, text) in [
        (
            &mut widgets.inactive,
            idle_fill,
            Color32::from_gray(138),
            ink,
        ),
        (&mut widgets.hovered, hover_fill, LIGHT_ACCENT, strong_ink),
        (
            &mut widgets.active,
            LIGHT_SELECTION,
            LIGHT_ACCENT,
            strong_ink,
        ),
        (&mut widgets.open, hover_fill, LIGHT_ACCENT, strong_ink),
    ] {
        state.bg_fill = fill;
        state.weak_bg_fill = fill;
        // Keep egui's stroke widths so resting controls only draw their fill.
        state.bg_stroke.color = border;
        state.fg_stroke.color = text;
    }
    visuals
}

pub(super) fn selected_fill(dark_mode: bool) -> egui::Color32 {
    if dark_mode {
        egui::Color32::from_rgb(48, 53, 62)
    } else {
        LIGHT_SELECTION
    }
}

pub(super) fn entry_kind_color(kind: EntryDisplayKind, dark_mode: bool) -> egui::Color32 {
    match (kind, dark_mode) {
        (EntryDisplayKind::Dir, true) => egui::Color32::from_rgb(52, 211, 153),
        (EntryDisplayKind::File, true) => egui::Color32::from_rgb(96, 165, 250),
        (EntryDisplayKind::Link, true) => egui::Color32::from_rgb(250, 204, 21),
        (EntryDisplayKind::Other, true) => egui::Color32::from_rgb(156, 163, 175),
        (EntryDisplayKind::Dir, false) => egui::Color32::from_rgb(30, 88, 56),
        (EntryDisplayKind::File, false) => egui::Color32::from_rgb(40, 76, 119),
        (EntryDisplayKind::Link, false) => egui::Color32::from_rgb(116, 65, 20),
        (EntryDisplayKind::Other, false) => egui::Color32::from_rgb(75, 85, 99),
    }
}

pub(super) fn highlight_text_color(dark_mode: bool) -> egui::Color32 {
    if dark_mode {
        egui::Color32::from_rgb(245, 158, 11)
    } else {
        egui::Color32::from_rgb(134, 56, 15)
    }
}

pub(super) fn current_row_marker_color(dark_mode: bool) -> egui::Color32 {
    if dark_mode {
        egui::Color32::LIGHT_BLUE
    } else {
        LIGHT_ACCENT
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
    ctx.set_visuals_of(egui::Theme::Light, light_visuals());
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

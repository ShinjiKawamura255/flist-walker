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
    }
}

pub(super) fn highlight_text_color() -> egui::Color32 {
    egui::Color32::from_rgb(245, 158, 11)
}

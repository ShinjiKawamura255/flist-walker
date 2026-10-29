use eframe::egui;

pub(in crate::app) fn centered_top_panel_label(
    ui: &mut egui::Ui,
    text: impl Into<String>,
) -> egui::Response {
    let text = text.into();
    let font_id = egui::TextStyle::Body.resolve(ui.style());
    let galley = ui
        .painter()
        .layout_no_wrap(text, font_id, ui.visuals().text_color());
    let desired_size = egui::vec2(
        galley.size().x,
        ui.spacing().interact_size.y.max(galley.size().y),
    );
    let (rect, response) = ui.allocate_exact_size(desired_size, egui::Sense::hover());
    if ui.is_rect_visible(rect) {
        let text_pos = egui::pos2(rect.left(), rect.center().y - (galley.size().y / 2.0));
        ui.painter()
            .galley(text_pos, galley, ui.visuals().text_color());
    }
    response
}

fn paint_compact_row_text(ui: &egui::Ui, rect: egui::Rect, text: &str, color: egui::Color32) {
    let font_id = egui::TextStyle::Body.resolve(ui.style());
    let galley = ui.painter().layout_no_wrap(text.to_owned(), font_id, color);
    let text_pos = egui::pos2(rect.left(), rect.center().y - (galley.size().y / 2.0));
    // The selected value may be wider at larger font sizes. Keep it before
    // the arrow while preserving vertical room for platform glyph metrics.
    let clip = egui::Rect::from_min_max(
        egui::pos2(rect.left(), ui.clip_rect().top()),
        egui::pos2(rect.right(), ui.clip_rect().bottom()),
    )
    .intersect(ui.clip_rect());
    ui.painter()
        .with_clip_rect(clip)
        .galley(text_pos, galley, color);
}

pub(in crate::app) fn paint_compact_combo_selected_text(
    ui: &egui::Ui,
    response: &egui::Response,
    text: &str,
) {
    let inner_rect = response.rect.shrink2(ui.spacing().button_padding);
    let icon_reserved = ui.spacing().icon_width + ui.spacing().icon_spacing;
    let text_rect = egui::Rect::from_min_max(
        inner_rect.left_top(),
        egui::pos2(
            (inner_rect.right() - icon_reserved).max(inner_rect.left()),
            inner_rect.bottom(),
        ),
    );
    let visuals = ui.style().interact(response);
    paint_compact_row_text(ui, text_rect, text, visuals.text_color());
}

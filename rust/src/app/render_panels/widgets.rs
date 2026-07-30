use eframe::egui;

const COMPACT_ROW_TEXT_Y_OFFSET: f32 = 2.0;

pub(in crate::app) fn centered_top_panel_label(
    ui: &mut egui::Ui,
    text: impl Into<String>,
) -> egui::Response {
    let text = text.into();
    let font_id = egui::TextStyle::Button.resolve(ui.style());
    let galley = ui
        .painter()
        .layout_no_wrap(text, font_id, ui.visuals().text_color());
    let desired_size = egui::vec2(
        galley.size().x,
        ui.spacing().interact_size.y.max(galley.size().y),
    );
    let (rect, response) = ui.allocate_exact_size(desired_size, egui::Sense::hover());
    if ui.is_rect_visible(rect) {
        let text_pos = egui::pos2(
            rect.left(),
            rect.center().y - (galley.size().y / 2.0) + COMPACT_ROW_TEXT_Y_OFFSET,
        );
        ui.painter()
            .galley(text_pos, galley, ui.visuals().text_color());
    }
    response
}

fn paint_compact_row_text(ui: &egui::Ui, rect: egui::Rect, text: &str, color: egui::Color32) {
    let font_id = egui::TextStyle::Button.resolve(ui.style());
    let galley = ui.painter().layout_no_wrap(text.to_owned(), font_id, color);
    let text_pos = egui::pos2(
        rect.left(),
        rect.center().y - (galley.size().y / 2.0) + COMPACT_ROW_TEXT_Y_OFFSET,
    );
    ui.painter().galley(text_pos, galley, color);
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

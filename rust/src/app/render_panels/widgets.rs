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

// Settings labels can wrap. Center the icon on the first line's visible ink;
// font line boxes alone shift the icon when platform font metrics change.
fn aligned_choice(ui: &mut egui::Ui, checked: bool, label: &str, radio: bool) -> egui::Response {
    let font = egui::TextStyle::Button.resolve(ui.style());
    let icon_width = ui.spacing().icon_width;
    let gap = ui.spacing().icon_spacing;
    let galley = ui.painter().layout(
        label.to_owned(),
        font,
        ui.visuals().text_color(),
        (ui.available_width() - icon_width - gap).max(1.0),
    );
    let ink = if galley.mesh_bounds.is_finite() && galley.mesh_bounds.is_positive() {
        galley.mesh_bounds
    } else {
        galley.rect
    };
    let first = galley
        .rows
        .first()
        .map(|row| {
            let bounds = row.visuals.mesh_bounds.translate(row.pos.to_vec2());
            if bounds.is_finite() && bounds.is_positive() {
                bounds
            } else {
                row.rect()
            }
        })
        .unwrap_or(ink);
    let bounds = galley.rect.union(ink).union(egui::Rect::from_center_size(
        egui::pos2(0.0, first.center().y),
        egui::Vec2::splat(icon_width),
    ));
    let size = egui::vec2(
        icon_width + gap + galley.size().x.max(ink.right()),
        bounds.height().max(ui.spacing().interact_size.y),
    );
    let (rect, mut response) = ui.allocate_exact_size(size, egui::Sense::click());
    if response.clicked() && (!radio || !checked) {
        response.mark_changed();
    }
    let selected = if response.changed() {
        !checked
    } else {
        checked
    };
    response.widget_info(|| {
        egui::WidgetInfo::selected(
            if radio {
                egui::WidgetType::RadioButton
            } else {
                egui::WidgetType::Checkbox
            },
            ui.is_enabled(),
            selected,
            label,
        )
    });
    if ui.is_rect_visible(rect) {
        let origin = egui::pos2(
            rect.left() + icon_width + gap,
            rect.top() + (rect.height() - bounds.height()) / 2.0 - bounds.top(),
        );
        let center = egui::pos2(rect.left() + icon_width / 2.0, origin.y + first.center().y);
        if radio {
            let visuals = ui.style().interact(&response);
            ui.painter()
                .circle(center, icon_width / 2.0, visuals.bg_fill, visuals.bg_stroke);
            if selected {
                ui.painter().circle_filled(
                    center,
                    ui.spacing().icon_width_inner / 3.0,
                    visuals.fg_stroke.color,
                );
            }
            ui.painter().galley(origin, galley, visuals.text_color());
        } else {
            let style = ui.style().checkbox_style(
                &egui::widget_style::Classes::default(),
                response.widget_state(),
            );
            let icon = egui::Rect::from_center_size(center, egui::Vec2::splat(style.checkbox_size));
            ui.painter().rect(
                icon.expand(style.checkbox_frame.inner_margin.left.into()),
                style.checkbox_frame.corner_radius,
                style.checkbox_frame.fill,
                style.checkbox_frame.stroke,
                egui::StrokeKind::Inside,
            );
            if selected {
                let check =
                    egui::Rect::from_center_size(center, egui::Vec2::splat(style.check_size));
                ui.painter().add(egui::Shape::line(
                    vec![
                        egui::pos2(check.left(), check.center().y),
                        egui::pos2(check.center().x, check.bottom()),
                        egui::pos2(check.right(), check.top()),
                    ],
                    style.check_stroke,
                ));
            }
            ui.painter().galley(origin, galley, style.text_style.color);
        }
        if response.has_focus() {
            ui.painter().rect_stroke(
                rect.expand(1.0),
                2.0,
                ui.visuals().selection.stroke,
                egui::StrokeKind::Inside,
            );
        }
    }
    response
}

pub(in crate::app) fn aligned_checkbox(
    ui: &mut egui::Ui,
    checked: &mut bool,
    label: &str,
) -> egui::Response {
    let response = aligned_choice(ui, *checked, label, false);
    if response.changed() {
        *checked = !*checked;
    }
    response
}

pub(in crate::app) fn aligned_radio_value<T: PartialEq>(
    ui: &mut egui::Ui,
    value: &mut T,
    selected: T,
    label: &str,
) -> egui::Response {
    let response = aligned_choice(ui, *value == selected, label, true);
    if response.changed() {
        *value = selected;
    }
    response
}

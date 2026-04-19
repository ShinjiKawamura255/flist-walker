use super::{
    render::{RenderCommand, RenderTabBarCommand},
    render_theme, FlistWalkerApp, TabAccentColor, TabAccentPalette, TabDragState,
};
use eframe::egui;

pub(super) fn render_tab_bar(app: &mut FlistWalkerApp, ui: &mut egui::Ui) {
    ui.horizontal(|ui| {
        let mut switch_to: Option<usize> = None;
        let mut close_tab: Option<usize> = None;
        let mut reorder_tab: Option<(usize, usize)> = None;
        let mut drag_state = app.shell.ui.tab_drag_state;
        let mut tab_rects: Vec<egui::Rect> = Vec::with_capacity(app.shell.tabs.len());
        for i in 0..app.shell.tabs.len() {
            let is_drag_source = drag_state.is_some_and(|state| state.source_index == i);
            let is_drop_target = drag_state.is_some_and(|state| state.hover_index == i);
            let is_active = app.shell.tabs.active_tab_index() == i;
            let tab_accent: Option<TabAccentColor> = app.shell.tabs.get(i).and_then(|tab| tab.tab_accent);
            let accent_palette = tab_accent.map(|accent| accent.palette(ui.visuals().dark_mode));
            let active_fill = render_theme::selected_fill(ui.visuals().dark_mode);
            let drag_fill = ui.visuals().selection.bg_fill.gamma_multiply(0.35);
            let drop_fill = ui.visuals().selection.bg_fill.gamma_multiply(0.18);
            let frame_fill = if is_drag_source {
                drag_fill
            } else if is_drop_target {
                drop_fill
            } else if is_active {
                accent_palette
                    .map(|palette| palette.background)
                    .unwrap_or(active_fill)
            } else {
                egui::Color32::TRANSPARENT
            };
            let frame_stroke = if is_drag_source || is_drop_target {
                ui.visuals().selection.stroke
            } else if let Some(palette) = accent_palette {
                egui::Stroke::new(
                    if is_active {
                        FlistWalkerApp::TAB_ACTIVE_BORDER_WIDTH
                    } else {
                        FlistWalkerApp::TAB_INACTIVE_BORDER_WIDTH
                    },
                    palette
                        .border
                        .gamma_multiply(if is_active { 1.0 } else { 0.72 }),
                )
            } else {
                let palette = TabAccentPalette::clear_outline(ui.visuals().dark_mode);
                egui::Stroke::new(
                    if is_active {
                        FlistWalkerApp::TAB_ACTIVE_BORDER_WIDTH
                    } else {
                        FlistWalkerApp::TAB_INACTIVE_BORDER_WIDTH
                    },
                    palette
                        .border
                        .gamma_multiply(if is_active { 1.0 } else { 0.82 }),
                )
            };
            let tab_response = egui::Frame::none()
                .fill(frame_fill)
                .stroke(frame_stroke)
                .rounding(egui::Rounding::same(FlistWalkerApp::TAB_ROUNDING))
                .inner_margin(egui::Margin::symmetric(6.0, 2.0))
                .show(ui, |ui| {
                    let title = app
                        .shell
                        .tabs
                        .get(i)
                        .map(|tab| app.tab_title(tab, i))
                        .unwrap_or_else(|| format!("Tab {}", i + 1));
                    let title_response = ui.add(
                        egui::Button::new(egui::RichText::new(title).strong().color(
                            if let Some(palette) = accent_palette {
                                if is_active {
                                    palette.foreground
                                } else {
                                    palette.foreground.gamma_multiply(0.92)
                                }
                            } else if is_active {
                                ui.visuals().strong_text_color()
                            } else {
                                ui.visuals().text_color().gamma_multiply(0.78)
                            },
                        ))
                        .frame(false)
                        .sense(egui::Sense::click_and_drag()),
                    );
                    let close_response = ui
                        .add_enabled(
                            app.shell.tabs.len() > 1,
                            egui::Button::new(egui::RichText::new("×").color(
                                accent_palette
                                    .map(|palette| {
                                        if is_active {
                                            palette.foreground
                                        } else {
                                            palette.border
                                        }
                                    })
                                    .unwrap_or_else(|| ui.visuals().text_color()),
                            ))
                            .small()
                            .frame(false),
                        )
                        .on_hover_text("Close tab");

                    if title_response.drag_started() {
                        drag_state = Some(TabDragState {
                            source_index: i,
                            hover_index: i,
                            press_pos: ui
                                .ctx()
                                .pointer_interact_pos()
                                .unwrap_or(title_response.rect.center()),
                            dragging: false,
                        });
                    }
                    if title_response.clicked_by(egui::PointerButton::Middle) {
                        close_tab = Some(i);
                    } else if title_response.clicked() {
                        switch_to = Some(i);
                    }
                    if close_response.clicked() {
                        close_tab = Some(i);
                    }

                    title_response.union(close_response)
                });
            let tab_interaction = tab_response.inner;
            paint_tab_accent_decoration(
                ui,
                tab_response.response.rect,
                accent_palette,
                is_active,
                is_drag_source,
                is_drop_target,
            );
            tab_interaction.context_menu(|ui| {
                render_tab_accent_menu(app, ui, i, tab_accent);
            });
            tab_rects.push(tab_response.response.rect);
        }
        if let Some(mut state) = drag_state {
            let pointer_pos = ui
                .ctx()
                .pointer_interact_pos()
                .or_else(|| ui.input(|i| i.pointer.latest_pos()));
            let primary_down = ui.ctx().input(|i| i.pointer.primary_down());

            if let Some((from_index, to_index)) =
                update_tab_drag_state(app, &mut state, &tab_rects, pointer_pos, primary_down)
            {
                reorder_tab = Some((from_index, to_index));
            }

            if state.dragging {
                if let Some(target_rect) = tab_rects.get(state.hover_index) {
                    let indicator_x = target_rect.left();
                    let indicator_top = target_rect.top() - 3.0;
                    let indicator_bottom = target_rect.bottom() + 3.0;
                    let stroke = egui::Stroke::new(3.0, ui.visuals().selection.stroke.color);
                    ui.painter().line_segment(
                        [
                            egui::pos2(indicator_x, indicator_top),
                            egui::pos2(indicator_x, indicator_bottom),
                        ],
                        stroke,
                    );
                    ui.painter().circle_filled(
                        egui::pos2(indicator_x, indicator_top),
                        4.0,
                        ui.visuals().selection.stroke.color,
                    );
                    ui.output_mut(|o| o.cursor_icon = egui::CursorIcon::Grabbing);
                }
            }
        }
        if ui
            .button("+")
            .on_hover_text(format!("New tab ({}+T)", FlistWalkerApp::primary_shortcut_label()))
            .clicked()
        {
            app.queue_render_command(RenderCommand::TabBar(RenderTabBarCommand::CreateNewTab));
            return;
        }
        if let Some(index) = close_tab {
            app.queue_render_command(RenderCommand::TabBar(RenderTabBarCommand::CloseTab(index)));
            return;
        }
        if let Some((from_index, to_index)) = reorder_tab {
            app.queue_render_command(RenderCommand::TabBar(RenderTabBarCommand::MoveTab {
                from_index,
                to_index,
            }));
            return;
        }
        if let Some(idx) = switch_to {
            app.queue_render_command(RenderCommand::TabBar(RenderTabBarCommand::SwitchToTab(idx)));
        }
    });
}

pub(super) fn paint_tab_accent_decoration(
    ui: &egui::Ui,
    rect: egui::Rect,
    accent_palette: Option<TabAccentPalette>,
    is_active: bool,
    is_drag_source: bool,
    is_drop_target: bool,
) {
    if is_drag_source || is_drop_target {
        return;
    }
    let Some(palette) = accent_palette else {
        return;
    };
    if is_active {
        return;
    }

    let glow_rect = egui::Rect::from_min_max(
        egui::pos2(
            rect.left() + 2.0,
            rect.bottom() - FlistWalkerApp::TAB_ACCENT_GLOW_HEIGHT,
        ),
        egui::pos2(rect.right() - 2.0, rect.bottom() - 1.0),
    );
    let line_rect = egui::Rect::from_min_max(
        egui::pos2(
            rect.left() + 4.0,
            rect.bottom() - FlistWalkerApp::TAB_ACCENT_LINE_HEIGHT - 1.0,
        ),
        egui::pos2(rect.right() - 4.0, rect.bottom() - 1.0),
    );
    ui.painter().rect_filled(
        glow_rect,
        egui::Rounding::same(FlistWalkerApp::TAB_ACCENT_LINE_HEIGHT),
        palette
            .background
            .gamma_multiply(if ui.visuals().dark_mode { 0.72 } else { 0.62 }),
    );
    ui.painter().rect_filled(
        line_rect,
        egui::Rounding::same(FlistWalkerApp::TAB_ACCENT_LINE_HEIGHT),
        palette.border,
    );
}

pub(super) fn render_tab_accent_menu(
    app: &mut FlistWalkerApp,
    ui: &mut egui::Ui,
    index: usize,
    current_accent: Option<TabAccentColor>,
) {
    ui.set_min_width(220.0);
    ui.label("Tab Color");
    if ui.button("Clear").clicked() {
        app.queue_render_command(RenderCommand::TabBar(RenderTabBarCommand::ClearTabAccent(index)));
        ui.close_menu();
        return;
    }
    ui.separator();
    ui.horizontal_wrapped(|ui| {
        for accent in TabAccentColor::ALL {
            let palette = accent.palette(ui.visuals().dark_mode);
            let mut button =
                egui::Button::new(egui::RichText::new(accent.label()).color(palette.foreground))
                    .fill(palette.background)
                    .stroke(egui::Stroke::new(
                        if current_accent == Some(accent) { 2.0 } else { 1.0 },
                        palette.border,
                    ))
                    .rounding(egui::Rounding::same(6.0));
            if current_accent == Some(accent) {
                button = button.min_size(egui::vec2(86.0, 24.0));
            }
            if ui.add(button).clicked() {
                app.queue_render_command(RenderCommand::TabBar(RenderTabBarCommand::SetTabAccent {
                    index,
                    accent,
                }));
                ui.close_menu();
            }
        }
    });
}

pub(super) fn tab_drop_index(tab_rects: &[egui::Rect], pointer_pos: egui::Pos2) -> Option<usize> {
    if tab_rects.is_empty() {
        return None;
    }
    for (index, rect) in tab_rects.iter().enumerate() {
        if pointer_pos.x < rect.center().x {
            return Some(index);
        }
    }
    Some(tab_rects.len().saturating_sub(1))
}

pub(super) fn update_tab_drag_state(
    app: &mut FlistWalkerApp,
    state: &mut TabDragState,
    tab_rects: &[egui::Rect],
    pointer_pos: Option<egui::Pos2>,
    primary_down: bool,
) -> Option<(usize, usize)> {
    if primary_down {
        if let Some(pointer_pos) = pointer_pos {
            if !state.dragging
                && pointer_pos.distance(state.press_pos) >= FlistWalkerApp::TAB_DRAG_START_DISTANCE
            {
                state.dragging = true;
            }
            if state.dragging {
                if let Some(target) = tab_drop_index(tab_rects, pointer_pos) {
                    state.hover_index = target;
                }
            }
        }
        app.shell.ui.tab_drag_state = Some(*state);
        return None;
    }

    app.shell.ui.tab_drag_state = None;
    state
        .dragging
        .then_some((state.source_index, state.hover_index))
}

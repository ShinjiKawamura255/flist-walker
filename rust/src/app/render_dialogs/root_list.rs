use crate::app::FlistWalkerApp;
use eframe::egui;

pub(super) fn render(app: &mut FlistWalkerApp, ctx: &egui::Context) {
    if !app.shell.features.root_browser.manage_list.open {
        return;
    }

    let mut add_input = false;
    let mut browse_and_add = false;
    let mut start_edit = false;
    let mut save_edit = false;
    let mut cancel_edit = false;
    let mut enter_remove_mode = false;
    let mut remove_selected = false;
    let mut cancel_remove_mode = false;
    let mut apply = false;
    let mut ok = false;
    let mut cancel = false;
    let viewport_id = FlistWalkerApp::manage_root_list_viewport_id();
    let parent_rect = ctx.input(|input| input.viewport().outer_rect);
    let viewport_builder = FlistWalkerApp::manage_root_list_viewport_builder(parent_rect);

    ctx.show_viewport_immediate(viewport_id, viewport_builder, |ui, _class| {
        if ui.input(|input| {
            input
                .viewport()
                .events
                .contains(&egui::ViewportEvent::Close)
        }) {
            cancel = true;
        }

        egui::CentralPanel::default().show(ui, |ui| {
            let mut add_response = None;
            ui.horizontal(|ui| {
                let row_height = ui.spacing().interact_size.y;
                let browse_width = 84.0;
                let add_width = 52.0;
                let spacing = ui.spacing().item_spacing.x * 2.0;
                let input_width =
                    (ui.available_width() - browse_width - add_width - spacing).max(160.0);
                let has_error = !app
                    .shell
                    .features
                    .root_browser
                    .manage_list
                    .add_error
                    .is_empty();
                let response = FlistWalkerApp::manage_root_list_text_edit(
                    ui,
                    &mut app.shell.features.root_browser.manage_list.input_path,
                    input_width,
                    has_error,
                    Some("Folder path"),
                );
                if response.changed() {
                    app.clear_manage_root_list_add_error();
                }
                add_response = Some(response);
                if ui
                    .add_sized([browse_width, row_height], egui::Button::new("Browse..."))
                    .clicked()
                {
                    browse_and_add = true;
                }
                if ui
                    .add_sized([add_width, row_height], egui::Button::new("Add"))
                    .clicked()
                {
                    add_input = true;
                }
            });
            if let Some(response) = add_response {
                let manage = &mut app.shell.features.root_browser.manage_list;
                let text = manage.input_path.clone();
                FlistWalkerApp::apply_manage_root_list_text_edit_focus(
                    &response,
                    &text,
                    &mut manage.add_focus_requested,
                    &mut manage.add_select_all_requested,
                );
            }
            let add_error = app
                .shell
                .features
                .root_browser
                .manage_list
                .add_error
                .clone();
            if !add_error.is_empty() {
                FlistWalkerApp::manage_root_list_error_label(ui, &add_error);
            }

            let notice = app.shell.features.root_browser.manage_list.notice.clone();
            if !notice.is_empty() {
                ui.label(notice);
            }

            ui.separator();
            ui.horizontal(|ui| {
                let manage = &app.shell.features.root_browser.manage_list;
                ui.heading(if manage.remove_mode {
                    "Select roots to remove"
                } else {
                    "Saved roots"
                });
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if manage.remove_mode {
                        if ui.button("Cancel").clicked() {
                            cancel_remove_mode = true;
                        }
                        if ui
                            .add_enabled(
                                !manage.selected_indices.is_empty(),
                                egui::Button::new("Remove selected"),
                            )
                            .clicked()
                        {
                            remove_selected = true;
                        }
                    } else {
                        if ui
                            .add_enabled(
                                manage.editing_index.is_none() && !manage.draft_roots.is_empty(),
                                egui::Button::new("Remove..."),
                            )
                            .clicked()
                        {
                            enter_remove_mode = true;
                        }
                        if ui
                            .add_enabled(
                                manage.selected_index.is_some() && manage.editing_index.is_none(),
                                egui::Button::new("Edit"),
                            )
                            .clicked()
                        {
                            start_edit = true;
                        }
                    }
                });
            });

            let button_row_height = ui.spacing().interact_size.y + ui.spacing().item_spacing.y;
            let list_height = (ui.available_height() - button_row_height - 8.0).max(80.0);
            egui::ScrollArea::vertical()
                .auto_shrink([false, false])
                .max_height(list_height)
                .show(ui, |ui| {
                    let roots = app
                        .shell
                        .features
                        .root_browser
                        .manage_list
                        .draft_roots
                        .clone();
                    if roots.is_empty() {
                        ui.label("No saved roots");
                    }
                    for (index, root) in roots.iter().enumerate() {
                        let label = root.to_string_lossy().to_string();
                        let remove_mode = app.shell.features.root_browser.manage_list.remove_mode;
                        let editing_index =
                            app.shell.features.root_browser.manage_list.editing_index;
                        if remove_mode {
                            let mut selected = app
                                .shell
                                .features
                                .root_browser
                                .manage_list
                                .selected_indices
                                .contains(&index);
                            if ui.checkbox(&mut selected, label).changed() {
                                let selected_indices = &mut app
                                    .shell
                                    .features
                                    .root_browser
                                    .manage_list
                                    .selected_indices;
                                if selected {
                                    selected_indices.insert(index);
                                } else {
                                    selected_indices.remove(&index);
                                }
                            }
                        } else if editing_index == Some(index) {
                            ui.vertical(|ui| {
                                let mut edit_response = None;
                                ui.horizontal(|ui| {
                                    let available = (ui.available_width() - 124.0).max(160.0);
                                    let has_error = !app
                                        .shell
                                        .features
                                        .root_browser
                                        .manage_list
                                        .edit_error
                                        .is_empty();
                                    let response = FlistWalkerApp::manage_root_list_text_edit(
                                        ui,
                                        &mut app.shell.features.root_browser.manage_list.edit_path,
                                        available,
                                        has_error,
                                        None,
                                    );
                                    if response.changed() {
                                        app.clear_manage_root_list_edit_error();
                                    }
                                    if response.has_focus() {
                                        if ui.input(|input| input.key_pressed(egui::Key::Enter)) {
                                            save_edit = true;
                                        }
                                        if ui.input(|input| input.key_pressed(egui::Key::Escape)) {
                                            cancel_edit = true;
                                        }
                                    }
                                    if ui.button("Save").clicked() {
                                        save_edit = true;
                                    }
                                    if ui.button("Cancel").clicked() {
                                        cancel_edit = true;
                                    }
                                    edit_response = Some(response);
                                });
                                if let Some(response) = edit_response {
                                    let manage = &mut app.shell.features.root_browser.manage_list;
                                    let text = manage.edit_path.clone();
                                    FlistWalkerApp::apply_manage_root_list_text_edit_focus(
                                        &response,
                                        &text,
                                        &mut manage.edit_focus_requested,
                                        &mut manage.edit_select_all_requested,
                                    );
                                }
                                let edit_error = app
                                    .shell
                                    .features
                                    .root_browser
                                    .manage_list
                                    .edit_error
                                    .clone();
                                if !edit_error.is_empty() {
                                    FlistWalkerApp::manage_root_list_error_label(ui, &edit_error);
                                }
                            });
                        } else {
                            let selected =
                                app.shell.features.root_browser.manage_list.selected_index
                                    == Some(index);
                            let response = FlistWalkerApp::manage_root_list_selectable_row(
                                ui, selected, &label,
                            );
                            if response.double_clicked() {
                                if app.select_manage_root_list_item(index) {
                                    start_edit = true;
                                }
                            } else if response.clicked() {
                                app.select_manage_root_list_item(index);
                            }
                        }
                    }
                });

            ui.separator();
            let action_height = ui.spacing().interact_size.y.round();
            let (row_rect, _) = ui.allocate_exact_size(
                egui::vec2(ui.available_width(), action_height),
                egui::Sense::hover(),
            );
            let [apply_rect, ok_rect, cancel_rect] =
                FlistWalkerApp::manage_root_list_action_button_rects(
                    row_rect,
                    action_height,
                    ui.spacing().item_spacing.x,
                );
            ui.scope(|ui| {
                let mut style = (**ui.style()).clone();
                style.visuals.widgets.hovered.expansion = 0.0;
                style.visuals.widgets.active.expansion = 0.0;
                style.visuals.widgets.open.expansion = 0.0;
                ui.set_style(style);
                if ui.put(apply_rect, egui::Button::new("Apply")).clicked() {
                    apply = true;
                }
                if ui.put(ok_rect, egui::Button::new("OK")).clicked() {
                    ok = true;
                }
                if ui.put(cancel_rect, egui::Button::new("Cancel")).clicked() {
                    cancel = true;
                }
            });
        });
    });

    if browse_and_add {
        app.queue_render_command(crate::app::render::RenderCommand::RootListDialog(
            crate::app::render::RenderRootListDialogCommand::BrowseAndAdd,
        ));
    }
    if add_input {
        app.queue_render_command(crate::app::render::RenderCommand::RootListDialog(
            crate::app::render::RenderRootListDialogCommand::AddInput,
        ));
    }
    if start_edit {
        app.queue_render_command(crate::app::render::RenderCommand::RootListDialog(
            crate::app::render::RenderRootListDialogCommand::StartEdit,
        ));
    }
    if save_edit {
        app.queue_render_command(crate::app::render::RenderCommand::RootListDialog(
            crate::app::render::RenderRootListDialogCommand::SaveEdit,
        ));
    }
    if cancel_edit {
        app.queue_render_command(crate::app::render::RenderCommand::RootListDialog(
            crate::app::render::RenderRootListDialogCommand::CancelEdit,
        ));
    }
    if enter_remove_mode {
        app.queue_render_command(crate::app::render::RenderCommand::RootListDialog(
            crate::app::render::RenderRootListDialogCommand::EnterRemoveMode,
        ));
    }
    if remove_selected {
        app.queue_render_command(crate::app::render::RenderCommand::RootListDialog(
            crate::app::render::RenderRootListDialogCommand::RemoveSelected,
        ));
    }
    if cancel_remove_mode {
        app.queue_render_command(crate::app::render::RenderCommand::RootListDialog(
            crate::app::render::RenderRootListDialogCommand::CancelRemoveMode,
        ));
    }
    if apply {
        app.queue_render_command(crate::app::render::RenderCommand::RootListDialog(
            crate::app::render::RenderRootListDialogCommand::Apply,
        ));
    }
    if ok {
        app.queue_render_command(crate::app::render::RenderCommand::RootListDialog(
            crate::app::render::RenderRootListDialogCommand::Ok,
        ));
    }
    if cancel {
        app.queue_render_command(crate::app::render::RenderCommand::RootListDialog(
            crate::app::render::RenderRootListDialogCommand::Cancel,
        ));
    }
}

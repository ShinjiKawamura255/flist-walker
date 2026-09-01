use crate::app::{
    render::{EmacsSinglelineOptions, RenderRootListDialogCommand},
    FlistWalkerApp,
};
use crate::path_utils::{normalize_path_for_display, normalize_text_for_display};
use eframe::egui;

#[derive(Default)]
pub(in crate::app) struct RootListRenderActions {
    pub(in crate::app) add_input: bool,
    pub(in crate::app) browse_and_add: bool,
    pub(in crate::app) start_edit: bool,
    pub(in crate::app) save_edit: bool,
    pub(in crate::app) cancel_edit: bool,
    pub(in crate::app) enter_remove_mode: bool,
    pub(in crate::app) remove_selected: bool,
    pub(in crate::app) cancel_remove_mode: bool,
    pub(in crate::app) apply: bool,
    pub(in crate::app) ok: bool,
    pub(in crate::app) cancel: bool,
}

pub(in crate::app) fn root_list_commands(
    actions: RootListRenderActions,
) -> Vec<RenderRootListDialogCommand> {
    let candidates = [
        (
            actions.browse_and_add,
            RenderRootListDialogCommand::BrowseAndAdd,
        ),
        (actions.add_input, RenderRootListDialogCommand::AddInput),
        (actions.start_edit, RenderRootListDialogCommand::StartEdit),
        (actions.save_edit, RenderRootListDialogCommand::SaveEdit),
        (actions.cancel_edit, RenderRootListDialogCommand::CancelEdit),
        (
            actions.enter_remove_mode,
            RenderRootListDialogCommand::EnterRemoveMode,
        ),
        (
            actions.remove_selected,
            RenderRootListDialogCommand::RemoveSelected,
        ),
        (
            actions.cancel_remove_mode,
            RenderRootListDialogCommand::CancelRemoveMode,
        ),
        (actions.apply, RenderRootListDialogCommand::Apply),
        (actions.ok, RenderRootListDialogCommand::Ok),
        (actions.cancel, RenderRootListDialogCommand::Cancel),
    ];
    candidates
        .into_iter()
        .filter_map(|(enabled, command)| enabled.then_some(command))
        .collect()
}

pub(super) fn render(app: &mut FlistWalkerApp, ctx: &egui::Context) {
    if !app.shell.features.root_browser.manage_list.open {
        return;
    }

    let mut actions = RootListRenderActions::default();
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
            actions.cancel = true;
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
                let emacs_enabled = app.shell.runtime.emacs_keybindings_enabled;
                let ime_composition_active = app.shell.ui.ime_composition_active;
                let response = {
                    let shell = &mut app.shell;
                    FlistWalkerApp::manage_root_list_text_edit(
                        ui,
                        &mut shell.features.root_browser.manage_list.input_path,
                        &mut shell.runtime.query_state.kill_buffer,
                        emacs_enabled,
                        ime_composition_active,
                        has_error,
                        EmacsSinglelineOptions::new(
                            Some(egui::Id::new("manage-root-list-add-path")),
                            input_width,
                            Some("Folder path"),
                        ),
                    )
                };
                if response.changed() {
                    app.clear_manage_root_list_add_error();
                }
                add_response = Some(response);
                if ui
                    .add_sized([browse_width, row_height], egui::Button::new("Browse..."))
                    .clicked()
                {
                    actions.browse_and_add = true;
                }
                if ui
                    .add_sized([add_width, row_height], egui::Button::new("Add"))
                    .clicked()
                {
                    actions.add_input = true;
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
                ui.label(normalize_text_for_display(&notice));
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
                            actions.cancel_remove_mode = true;
                        }
                        if ui
                            .add_enabled(
                                !manage.selected_indices.is_empty(),
                                egui::Button::new("Remove selected"),
                            )
                            .clicked()
                        {
                            actions.remove_selected = true;
                        }
                    } else {
                        if ui
                            .add_enabled(
                                manage.editing_index.is_none() && !manage.draft_roots.is_empty(),
                                egui::Button::new("Remove..."),
                            )
                            .clicked()
                        {
                            actions.enter_remove_mode = true;
                        }
                        if ui
                            .add_enabled(
                                manage.selected_index.is_some() && manage.editing_index.is_none(),
                                egui::Button::new("Edit"),
                            )
                            .clicked()
                        {
                            actions.start_edit = true;
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
                        let label = normalize_path_for_display(root);
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
                                    let emacs_enabled = app.shell.runtime.emacs_keybindings_enabled;
                                    let ime_composition_active =
                                        app.shell.ui.ime_composition_active;
                                    let response = {
                                        let shell = &mut app.shell;
                                        FlistWalkerApp::manage_root_list_text_edit(
                                            ui,
                                            &mut shell.features.root_browser.manage_list.edit_path,
                                            &mut shell.runtime.query_state.kill_buffer,
                                            emacs_enabled,
                                            ime_composition_active,
                                            has_error,
                                            EmacsSinglelineOptions::new(
                                                Some(egui::Id::new("manage-root-list-edit-path")),
                                                available,
                                                None,
                                            ),
                                        )
                                    };
                                    if response.changed() {
                                        app.clear_manage_root_list_edit_error();
                                    }
                                    if response.has_focus() {
                                        if ui.input(|input| input.key_pressed(egui::Key::Enter)) {
                                            actions.save_edit = true;
                                        }
                                        if ui.input(|input| input.key_pressed(egui::Key::Escape)) {
                                            actions.cancel_edit = true;
                                        }
                                    }
                                    if ui.button("Save").clicked() {
                                        actions.save_edit = true;
                                    }
                                    if ui.button("Cancel").clicked() {
                                        actions.cancel_edit = true;
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
                            let response = FlistWalkerApp::selectable_row(ui, selected, &label);
                            if response.double_clicked() {
                                if app.select_manage_root_list_item(index) {
                                    actions.start_edit = true;
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
                let validation_pending = app.shell.worker_bus.root_validation.in_progress;
                ui.add_enabled_ui(!validation_pending, |ui| {
                    if ui.put(apply_rect, egui::Button::new("Apply")).clicked() {
                        actions.apply = true;
                    }
                    if ui.put(ok_rect, egui::Button::new("OK")).clicked() {
                        actions.ok = true;
                    }
                });
                if ui.put(cancel_rect, egui::Button::new("Cancel")).clicked() {
                    actions.cancel = true;
                }
            });
        });
    });

    for command in root_list_commands(actions) {
        app.queue_render_command(crate::app::render::RenderCommand::RootListDialog(command));
    }
}

use super::super::FlistWalkerApp;
use eframe::egui;

impl FlistWalkerApp {
    pub(in crate::app) fn primary_shortcut_label() -> &'static str {
        #[cfg(target_os = "macos")]
        {
            "Cmd"
        }
        #[cfg(not(target_os = "macos"))]
        {
            "Ctrl"
        }
    }

    pub(in crate::app) fn handle_shortcuts(&mut self, ctx: &egui::Context) {
        let restore_query_after_held_escape = self.suppress_held_preview_exit(ctx);
        if self.handle_selection_inspector_shortcuts(ctx) {
            return;
        }
        if self.settings_dialog.is_open() {
            if self.consume_gui_cancel(ctx) {
                self.close_settings_dialog();
            }
            return;
        }
        if self.handle_previous_update_failure_shortcuts(ctx) {
            return;
        }
        if self.handle_update_install_failure_shortcuts(ctx) {
            return;
        }
        if self.handle_update_check_failure_shortcuts(ctx) {
            return;
        }
        if self.handle_filelist_dialog_shortcuts(ctx) {
            return;
        }
        if self.shell.ui.help_open && self.handle_help_dialog_shortcuts(ctx) {
            return;
        }
        if self.shell.features.presets.picker.open && self.handle_preset_picker_shortcuts(ctx) {
            return;
        }
        if self.handle_help_dialog_shortcuts(ctx) {
            return;
        }
        if self.handle_preset_picker_shortcuts(ctx) {
            return;
        }
        // Modal dispatch owns focus first. egui may already have surrendered
        // query focus for a held Escape before application event consumption.
        if restore_query_after_held_escape
            && !self.shell.runtime.query_state.is_history_search_active()
            && !self.is_root_dropdown_open(ctx)
            && !self.shell.ui.ime_composition_active
            && !ctx.input(|input| {
                input
                    .events
                    .iter()
                    .any(|event| matches!(event, egui::Event::Ime(_)))
            })
        {
            self.clear_unfocus_query_request();
            self.request_focus_query();
            ctx.memory_mut(|memory| memory.request_focus(self.shell.ui.query_input_id));
        }
        let query_focused = ctx.memory(|m| m.has_focus(self.shell.ui.query_input_id));
        self.handle_shortcuts_with_focus(ctx, query_focused);
    }

    fn handle_preset_picker_shortcuts(&mut self, ctx: &egui::Context) -> bool {
        if !self.shell.features.presets.picker.open {
            if Self::consume_gui_shortcut(ctx, egui::Key::P, true) {
                self.open_preset_picker(ctx);
                return true;
            }
            return false;
        }

        if self.shell.features.presets.picker.named_roots.open {
            if self.shell.features.presets.picker.named_roots.editor.open {
                if ctx
                    .input_mut(|input| input.consume_key(egui::Modifiers::NONE, egui::Key::Escape))
                    || self.consume_emacs_shortcut(ctx, egui::Key::G, false)
                {
                    self.cancel_named_root_edit();
                    return true;
                }
                if Self::consume_gui_shortcut(ctx, egui::Key::Enter, false) {
                    self.request_save_named_root();
                    return true;
                }
                return true;
            }
            if self
                .shell
                .features
                .presets
                .picker
                .named_roots
                .confirm_delete
            {
                if ctx
                    .input_mut(|input| input.consume_key(egui::Modifiers::NONE, egui::Key::Escape))
                    || self.consume_emacs_shortcut(ctx, egui::Key::G, false)
                {
                    self.cancel_delete_named_root();
                    return true;
                }
                return true;
            }
            if self.consume_gui_cancel(ctx) {
                self.close_named_root_manager();
                return true;
            }
            if self.consume_gui_next(ctx) {
                self.move_named_root_selection(1);
                return true;
            }
            if self.consume_gui_previous(ctx) {
                self.move_named_root_selection(-1);
                return true;
            }
            if ctx.input_mut(|input| input.consume_key(egui::Modifiers::NONE, egui::Key::F2)) {
                self.start_selected_named_root_edit();
                return true;
            }
            if ctx.input_mut(|input| input.consume_key(egui::Modifiers::NONE, egui::Key::Delete)) {
                self.start_selected_named_root_delete();
                return true;
            }
            return true;
        }

        if self.shell.features.presets.picker.editor.open {
            if self.consume_gui_cancel(ctx) {
                self.cancel_preset_edit();
                return true;
            }
            if Self::consume_gui_shortcut(ctx, egui::Key::Enter, false) {
                self.request_save_preset_edit();
                return true;
            }
            return true;
        }

        if self.shell.features.presets.picker.confirm_delete {
            if self.consume_gui_cancel(ctx) {
                self.cancel_delete_preset();
            }
            return true;
        }

        if self.consume_gui_cancel(ctx) {
            self.close_preset_picker();
            return true;
        }
        if self.consume_gui_next(ctx) {
            self.move_preset_picker_selection(1);
            return true;
        }
        if self.consume_gui_previous(ctx) {
            self.move_preset_picker_selection(-1);
            return true;
        }
        if self.consume_gui_accept(ctx) {
            self.apply_selected_preset();
            return true;
        }
        if ctx.input_mut(|input| input.consume_key(egui::Modifiers::NONE, egui::Key::F2)) {
            self.start_selected_preset_edit();
            return true;
        }
        // The picker text field may keep ordinary text/editing events, while application
        // shortcuts must not leak to the search/results view behind the modal.
        true
    }

    pub(in crate::app) fn consume_gui_shortcut(
        ctx: &egui::Context,
        key: egui::Key,
        shift: bool,
    ) -> bool {
        #[cfg(target_os = "macos")]
        {
            let primary = egui::Modifiers {
                mac_cmd: true,
                shift,
                ..Default::default()
            };
            if ctx.input_mut(|i| i.consume_key(primary, key)) {
                return true;
            }
            let fallback = egui::Modifiers {
                command: true,
                shift,
                ..Default::default()
            };
            ctx.input_mut(|i| i.consume_key(fallback, key))
        }
        #[cfg(not(target_os = "macos"))]
        {
            let mods = egui::Modifiers {
                ctrl: true,
                shift,
                ..Default::default()
            };
            ctx.input_mut(|i| i.consume_key(mods, key))
        }
    }

    // Consume held keys as well, but activate commands only on a new press.
    fn consume_preview_key(
        ctx: &egui::Context,
        key: egui::Key,
        modifiers: egui::Modifiers,
    ) -> (bool, bool) {
        ctx.input_mut(|input| {
            let mut seen = false;
            let mut fresh = false;
            input.events.retain(|event| {
                if let egui::Event::Key {
                    key: actual,
                    pressed: true,
                    repeat,
                    modifiers: actual_mods,
                    ..
                } = event
                {
                    if *actual == key
                        && actual_mods.matches_logically(modifiers)
                        && actual_mods.shift == modifiers.shift
                        && actual_mods.alt == modifiers.alt
                    {
                        seen = true;
                        fresh |= !repeat;
                        return false;
                    }
                }
                true
            });
            (seen, fresh)
        })
    }

    fn consume_preview_primary(ctx: &egui::Context, shift: bool) -> (bool, bool) {
        let modifiers = if cfg!(target_os = "macos") {
            egui::Modifiers {
                mac_cmd: true,
                shift,
                ..Default::default()
            }
        } else {
            egui::Modifiers {
                ctrl: true,
                shift,
                ..Default::default()
            }
        };
        let primary = Self::consume_preview_key(ctx, egui::Key::L, modifiers);
        if primary.0 || !cfg!(target_os = "macos") {
            return primary;
        }
        Self::consume_preview_key(
            ctx,
            egui::Key::L,
            egui::Modifiers {
                command: true,
                shift,
                ..Default::default()
            },
        )
    }

    // A key that returned from preview must finish its press before the same
    // key can reach query cancel/focus commands, including commands in modals.
    // Release processing is ordered so release + fresh press in one frame works.
    fn suppress_held_preview_exit(&mut self, ctx: &egui::Context) -> bool {
        let Some(key) = self.paged_preview_view.exit_key_held else {
            return false;
        };
        let mut held = true;
        let mut suppressed = false;
        ctx.input_mut(|input| {
            input.events.retain(|event| {
                if let egui::Event::Key {
                    key: actual,
                    pressed,
                    ..
                } = event
                {
                    if *actual == key && held {
                        if !pressed {
                            held = false;
                        } else {
                            suppressed = true;
                            return false;
                        }
                    }
                }
                true
            });
        });
        if !held || !ctx.input(|input| input.key_down(key)) {
            self.paged_preview_view.exit_key_held = None;
        }
        suppressed
            && key == egui::Key::Escape
            && ctx.memory(|memory| {
                memory.focused().is_none()
                    && memory.had_focus_last_frame(self.shell.ui.query_input_id)
            })
    }

    fn leave_preview_controls(&mut self, ctx: &egui::Context) {
        self.paged_preview_view.controls_focused = false;
        self.clear_unfocus_query_request();
        self.request_focus_query();
        ctx.memory_mut(|memory| memory.request_focus(self.shell.ui.query_input_id));
    }

    fn handle_preview_controls_shortcuts(
        &mut self,
        ctx: &egui::Context,
        query_focused: bool,
    ) -> bool {
        let blocked = self.shell.runtime.query_state.is_history_search_active()
            || self.shell.ui.ime_composition_active
            || ctx.input(|input| {
                input
                    .events
                    .iter()
                    .any(|event| matches!(event, egui::Event::Ime(_)))
            })
            || self.is_root_dropdown_open(ctx);
        if blocked {
            if self.paged_preview_view.controls_focused
                && (self.shell.ui.ime_composition_active
                    || ctx.input(|input| {
                        input
                            .events
                            .iter()
                            .any(|event| matches!(event, egui::Event::Ime(_)))
                    }))
            {
                return true;
            }
            return false;
        }
        if self.paged_preview_for_current().is_none() && !self.preview_controls_loading_current() {
            if self.paged_preview_view.controls_focused {
                self.leave_preview_controls(ctx);
            }
            return Self::consume_preview_primary(ctx, true).0;
        }
        if query_focused {
            self.paged_preview_view.controls_focused = false;
        }
        let (toggle_seen, toggle_fresh) = Self::consume_preview_primary(ctx, true);
        if toggle_seen {
            if toggle_fresh {
                if self.paged_preview_view.controls_focused {
                    self.paged_preview_view.exit_key_held = Some(egui::Key::L);
                    self.leave_preview_controls(ctx);
                } else {
                    self.paged_preview_view.controls_focused = true;
                    self.clear_focus_query_request();
                    self.request_unfocus_query();
                    ctx.memory_mut(|memory| memory.stop_text_input());
                }
            }
            return true;
        }
        if !self.paged_preview_view.controls_focused {
            return false;
        }
        for (key, (seen, fresh)) in [
            (egui::Key::L, Self::consume_preview_primary(ctx, false)),
            (
                egui::Key::Escape,
                Self::consume_preview_key(ctx, egui::Key::Escape, egui::Modifiers::NONE),
            ),
            (
                egui::Key::G,
                if self.shell.runtime.emacs_keybindings_enabled {
                    Self::consume_preview_key(
                        ctx,
                        egui::Key::G,
                        egui::Modifiers {
                            ctrl: true,
                            ..Default::default()
                        },
                    )
                } else {
                    (false, false)
                },
            ),
        ] {
            if seen {
                if fresh {
                    self.paged_preview_view.exit_key_held = Some(key);
                    self.leave_preview_controls(ctx);
                }
                return true;
            }
        }
        for (key, direction) in [(egui::Key::ArrowLeft, -1_isize), (egui::Key::ArrowRight, 1)] {
            if Self::consume_preview_key(ctx, key, egui::Modifiers::NONE).1 {
                let actions = super::super::paged_preview_flow::PreviewAction::ALL;
                let current = actions
                    .iter()
                    .position(|action| *action == self.paged_preview_view.selected_control)
                    .unwrap_or(0);
                self.paged_preview_view.selected_control = actions
                    [(current as isize + direction).rem_euclid(actions.len() as isize) as usize];
            }
        }
        let mut accept = Self::consume_preview_key(ctx, egui::Key::Enter, egui::Modifiers::NONE).1;
        accept |= Self::consume_preview_key(ctx, egui::Key::Space, egui::Modifiers::NONE).1;
        if self.shell.runtime.emacs_keybindings_enabled {
            let modifiers = egui::Modifiers {
                ctrl: true,
                ..Default::default()
            };
            accept |= Self::consume_preview_key(ctx, egui::Key::J, modifiers).1;
            accept |= Self::consume_preview_key(ctx, egui::Key::M, modifiers).1;
        }
        if accept {
            self.apply_preview_action(self.paged_preview_view.selected_control);
        }
        for (key, pages) in [(egui::Key::PageUp, -1), (egui::Key::PageDown, 1)] {
            if Self::consume_preview_key(ctx, key, egui::Modifiers::NONE).1 {
                self.paged_preview_view.scroll_pages += pages;
            }
        }
        if ctx.input_mut(|input| input.consume_key(egui::Modifiers::NONE, egui::Key::Tab))
            || ctx.input_mut(|input| input.consume_key(egui::Modifiers::SHIFT, egui::Key::Tab))
            || self.consume_emacs_shortcut(ctx, egui::Key::I, false)
        {
            self.toggle_pin_current_from_tab();
        }
        true
    }

    fn consume_copy_event_shortcut(ctx: &egui::Context) -> bool {
        let modifiers = ctx.input(|i| i.modifiers);
        #[cfg(target_os = "macos")]
        let primary_pressed = modifiers.mac_cmd || modifiers.command;
        #[cfg(not(target_os = "macos"))]
        let primary_pressed = modifiers.ctrl || modifiers.command;

        if !primary_pressed || !modifiers.shift {
            return false;
        }

        ctx.input_mut(|i| {
            let mut consumed = false;
            i.events.retain(|event| {
                let is_copy_event = matches!(event, egui::Event::Copy);
                consumed |= is_copy_event;
                !is_copy_event
            });
            consumed
        })
    }

    fn consume_ctrl_v_page_down_shortcut(&self, ctx: &egui::Context) -> bool {
        if !self.shell.runtime.emacs_keybindings_enabled {
            return false;
        }
        let ctrl_v_mods = egui::Modifiers {
            ctrl: true,
            ..Default::default()
        };
        if ctx.input_mut(|i| i.consume_key(ctrl_v_mods, egui::Key::V)) {
            return true;
        }

        let modifiers = ctx.input(|i| i.modifiers);
        if !modifiers.ctrl || modifiers.alt || modifiers.shift {
            return false;
        }

        ctx.input_mut(|i| {
            let mut consumed = false;
            i.events.retain(|event| {
                let is_paste_event = matches!(event, egui::Event::Paste(_));
                consumed |= is_paste_event;
                !is_paste_event
            });
            consumed
        })
    }

    pub(in crate::app) fn consume_tab_switch_shortcut(
        ctx: &egui::Context,
        key: egui::Key,
        shift: bool,
    ) -> bool {
        let mods = egui::Modifiers {
            ctrl: true,
            shift,
            ..Default::default()
        };
        ctx.input_mut(|i| i.consume_key(mods, key))
    }

    pub(in crate::app) fn consume_emacs_shortcut(
        &self,
        ctx: &egui::Context,
        key: egui::Key,
        shift: bool,
    ) -> bool {
        if !self.shell.runtime.emacs_keybindings_enabled {
            return false;
        }
        let mods = egui::Modifiers {
            ctrl: true,
            shift,
            ..Default::default()
        };
        if ctx.input_mut(|i| i.consume_key(mods, key)) {
            return true;
        }
        #[cfg(target_os = "macos")]
        {
            // Some backends may surface ctrl chords via command bit on macOS.
            let fallback = egui::Modifiers {
                command: true,
                ctrl: true,
                shift,
                ..Default::default()
            };
            ctx.input_mut(|i| i.consume_key(fallback, key))
        }
        #[cfg(not(target_os = "macos"))]
        false
    }

    // Regression guard: translate Emacs chords at the shared application-command
    // boundary so modal and main surfaces cannot silently diverge. Do not add a new
    // Up/Down/Enter/Escape handler without these semantic helpers and the paired
    // regression_emacs_* tests.
    pub(in crate::app) fn consume_gui_next(&self, ctx: &egui::Context) -> bool {
        self.consume_emacs_shortcut(ctx, egui::Key::N, false)
            || ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::ArrowDown))
    }

    pub(in crate::app) fn consume_gui_previous(&self, ctx: &egui::Context) -> bool {
        self.consume_emacs_shortcut(ctx, egui::Key::P, false)
            || ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::ArrowUp))
    }

    pub(in crate::app) fn consume_gui_accept(&self, ctx: &egui::Context) -> bool {
        self.consume_emacs_shortcut(ctx, egui::Key::J, false)
            || self.consume_emacs_shortcut(ctx, egui::Key::M, false)
            || ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Enter))
    }

    pub(in crate::app) fn consume_gui_cancel(&self, ctx: &egui::Context) -> bool {
        self.consume_emacs_shortcut(ctx, egui::Key::G, false)
            || ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Escape))
    }

    pub(in crate::app) fn handle_shortcuts_with_focus(
        &mut self,
        ctx: &egui::Context,
        query_focused: bool,
    ) {
        if Self::consume_gui_shortcut(ctx, egui::Key::R, true) {
            self.open_root_dropdown(ctx);
            return;
        }
        if self.is_root_dropdown_open(ctx) {
            if self.consume_gui_next(ctx) {
                self.move_root_dropdown_selection(1);
                return;
            }
            if self.consume_gui_previous(ctx) {
                self.move_root_dropdown_selection(-1);
                return;
            }
            if self.consume_gui_accept(ctx) {
                self.apply_root_dropdown_selection(ctx);
                return;
            }
            if self.consume_gui_cancel(ctx) {
                self.close_root_dropdown(ctx);
                return;
            }
        }

        if Self::consume_gui_shortcut(ctx, egui::Key::T, true) {
            self.restore_recently_closed_tab();
            return;
        }
        if Self::consume_gui_shortcut(ctx, egui::Key::T, false) {
            self.create_new_tab();
            return;
        }
        // Regression guard: focused query editing owns Ctrl+W before the global tab
        // shortcut. Do not reorder this below tab close or defer it to TextEdit, which
        // can consume the same event twice. Keep it paired with regression_ctrl_w_*.
        if self.consume_ctrl_w_search_edit(ctx, query_focused) {
            return;
        }
        let ctrl_w_reserved_for_search_edit = query_focused
            && self.shell.runtime.emacs_keybindings_enabled
            && self.shell.runtime.ctrl_w_deletes_word_in_query
            && !cfg!(target_os = "macos");
        if !ctrl_w_reserved_for_search_edit && Self::consume_gui_shortcut(ctx, egui::Key::W, false)
        {
            self.close_active_tab();
            return;
        }
        if Self::consume_tab_switch_shortcut(ctx, egui::Key::Tab, true) {
            self.activate_previous_tab();
            return;
        }
        if Self::consume_tab_switch_shortcut(ctx, egui::Key::Tab, false) {
            self.activate_next_tab();
            return;
        }
        for (shortcut_number, key) in [
            (1, egui::Key::Num1),
            (2, egui::Key::Num2),
            (3, egui::Key::Num3),
            (4, egui::Key::Num4),
            (5, egui::Key::Num5),
            (6, egui::Key::Num6),
            (7, egui::Key::Num7),
            (8, egui::Key::Num8),
            (9, egui::Key::Num9),
        ] {
            if Self::consume_gui_shortcut(ctx, key, false) {
                self.activate_tab_shortcut(shortcut_number);
                return;
            }
        }
        if self.handle_preview_controls_shortcuts(ctx, query_focused) {
            return;
        }
        // Regression guard: Primary+L is a focus toggle and must update the pending
        // focus flags before TextEdit is rendered. Keep this paired with
        // regression_primary_l_toggles_query_focus_through_full_frames.
        if Self::consume_gui_shortcut(ctx, egui::Key::L, false) {
            if query_focused {
                self.clear_focus_query_request();
                self.request_unfocus_query();
            } else {
                self.request_focus_query();
                self.clear_unfocus_query_request();
            }
            return;
        }
        if Self::consume_gui_shortcut(ctx, egui::Key::O, true) {
            self.browse_for_root_in_new_tab();
            return;
        }
        if Self::consume_gui_shortcut(ctx, egui::Key::O, false) {
            self.browse_for_root();
            return;
        }

        if self.shell.runtime.query_state.is_history_search_active() {
            if self.consume_gui_next(ctx) {
                self.move_history_search_selection(1);
            }
            if self.consume_gui_previous(ctx) {
                self.move_history_search_selection(-1);
            }
            if self.consume_gui_cancel(ctx) {
                self.cancel_history_search();
            }
            if self.consume_gui_accept(ctx) {
                self.accept_history_search();
            }
            if query_focused {
                ctx.memory_mut(|m| m.request_focus(self.shell.ui.query_input_id));
            }
            return;
        }

        if self.consume_emacs_shortcut(ctx, egui::Key::N, false) {
            self.move_row(1);
        }
        if self.consume_emacs_shortcut(ctx, egui::Key::P, false) {
            self.move_row(-1);
        }
        if self.consume_emacs_shortcut(ctx, egui::Key::R, false) {
            self.start_history_search();
            if query_focused {
                ctx.memory_mut(|m| m.request_focus(self.shell.ui.query_input_id));
            }
        }
        if Self::consume_gui_shortcut(ctx, egui::Key::C, true)
            || Self::consume_copy_event_shortcut(ctx)
        {
            // Regression guard: egui-winit may translate Ctrl/Cmd+Shift+C into
            // Event::Copy before widgets see Key::C; keep both paths as path-copy.
            self.shell.ui.pending_copy_shortcut = true;
        }
        if self.consume_gui_cancel(ctx) {
            self.clear_query_and_selection();
        }
        let tab_forward = ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Tab));
        if tab_forward {
            self.toggle_pin_current_from_tab();
            // Keep Tab dedicated to pin toggle without changing query focus active/inactive state.
            if query_focused {
                ctx.memory_mut(|m| m.request_focus(self.shell.ui.query_input_id));
            } else {
                ctx.memory_mut(|m| m.stop_text_input());
            }
        }
        let tab_backward = ctx.input_mut(|i| i.consume_key(egui::Modifiers::SHIFT, egui::Key::Tab));
        if tab_backward {
            self.toggle_pin_current_from_tab();
            // Keep Shift+Tab dedicated to pin toggle without changing query focus active/inactive state.
            if query_focused {
                ctx.memory_mut(|m| m.request_focus(self.shell.ui.query_input_id));
            } else {
                ctx.memory_mut(|m| m.stop_text_input());
            }
        }
        if self.consume_emacs_shortcut(ctx, egui::Key::I, false) {
            self.toggle_pin_current_from_tab();
        }
        if self.consume_gui_next(ctx) {
            self.move_row(1);
        }
        if self.consume_gui_previous(ctx) {
            self.move_row(-1);
        }
        if ctx.input_mut(|i| i.consume_key(egui::Modifiers::SHIFT, egui::Key::Enter)) {
            self.execute_selected_open_folder();
        }
        if self.consume_gui_accept(ctx) {
            self.execute_selected();
        }

        if self.shell.ui.ime_composition_active {
            return;
        }
        // Regression guard: query focus must not disable row movement/pin toggle/execute shortcuts.
        if query_focused {
            return;
        }

        if ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Home)) {
            self.move_to_first_row();
        }
        if ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::End)) {
            self.move_to_last_row();
        }
        if ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::PageUp)) {
            self.move_page(-1);
        }
        if ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::PageDown)) {
            self.move_page(1);
        }
        if self.consume_ctrl_v_page_down_shortcut(ctx) {
            self.move_page(1);
        }
        if self.shell.runtime.emacs_keybindings_enabled
            && ctx.input(|i| i.modifiers.alt && i.key_pressed(egui::Key::V))
        {
            self.move_page(-1);
        }
    }
}

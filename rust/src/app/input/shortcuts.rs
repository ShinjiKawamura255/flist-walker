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
        if self.handle_filelist_dialog_shortcuts(ctx) {
            return;
        }
        let query_focused = ctx.memory(|m| m.has_focus(self.shell.ui.query_input_id));
        self.handle_shortcuts_with_focus(ctx, query_focused);
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
            return ctx.input_mut(|i| i.consume_key(fallback, key));
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
            return ctx.input_mut(|i| i.consume_key(fallback, key));
        }
        #[cfg(not(target_os = "macos"))]
        false
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
            if self.consume_emacs_shortcut(ctx, egui::Key::N, false)
                || ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::ArrowDown))
            {
                self.move_root_dropdown_selection(1);
                return;
            }
            if self.consume_emacs_shortcut(ctx, egui::Key::P, false)
                || ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::ArrowUp))
            {
                self.move_root_dropdown_selection(-1);
                return;
            }
            if self.consume_emacs_shortcut(ctx, egui::Key::J, false)
                || self.consume_emacs_shortcut(ctx, egui::Key::M, false)
                || ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Enter))
            {
                self.apply_root_dropdown_selection(ctx);
                return;
            }
            if self.consume_emacs_shortcut(ctx, egui::Key::G, false)
                || ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Escape))
            {
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
        if Self::consume_gui_shortcut(ctx, egui::Key::W, false) {
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
            if self.consume_emacs_shortcut(ctx, egui::Key::N, false) {
                self.move_history_search_selection(1);
            }
            if self.consume_emacs_shortcut(ctx, egui::Key::P, false) {
                self.move_history_search_selection(-1);
            }
            if self.consume_emacs_shortcut(ctx, egui::Key::G, false)
                || ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Escape))
            {
                self.cancel_history_search();
            }
            if ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::ArrowDown)) {
                self.move_history_search_selection(1);
            }
            if ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::ArrowUp)) {
                self.move_history_search_selection(-1);
            }
            if self.consume_emacs_shortcut(ctx, egui::Key::J, false)
                || self.consume_emacs_shortcut(ctx, egui::Key::M, false)
                || ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Enter))
            {
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
        if self.consume_emacs_shortcut(ctx, egui::Key::G, false)
            || ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Escape))
        {
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
        if ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::ArrowDown)) {
            self.move_row(1);
        }
        if ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::ArrowUp)) {
            self.move_row(-1);
        }
        if self.consume_emacs_shortcut(ctx, egui::Key::J, false)
            || self.consume_emacs_shortcut(ctx, egui::Key::M, false)
        {
            self.execute_selected();
        }
        if ctx.input_mut(|i| i.consume_key(egui::Modifiers::SHIFT, egui::Key::Enter)) {
            self.execute_selected_open_folder();
        }
        if ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Enter)) {
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

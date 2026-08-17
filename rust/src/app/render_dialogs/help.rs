use crate::app::{render::RenderHelpDialogCommand, FlistWalkerApp};
use eframe::egui;

impl FlistWalkerApp {
    pub(in crate::app) fn gui_help_lines(emacs_keybindings_enabled: bool) -> Vec<String> {
        let primary = Self::primary_shortcut_label();
        let mut lines = vec![
            "Search and navigation".to_string(),
            "Up / Down — Move the current row".to_string(),
            "PageUp / PageDown — Move one result page".to_string(),
            format!("{primary}+L — Focus or leave the search box"),
            "".to_string(),
            "Selection and actions".to_string(),
            "Enter — Open or execute the current row and pinned items".to_string(),
            "Shift+Enter — Reveal the current row or pinned items in their folders".to_string(),
            "Tab / Shift+Tab — Toggle pin on the current row".to_string(),
            format!("{primary}+Shift+C — Copy the current or pinned paths"),
            "Esc — Clear the query and pinned items".to_string(),
            "".to_string(),
            "Tabs and roots".to_string(),
            format!("{primary}+T / {primary}+W — Create or close a tab"),
            format!("{primary}+Shift+T — Restore the most recently closed tab"),
            "Ctrl+Tab / Ctrl+Shift+Tab — Switch tabs".to_string(),
            format!("{primary}+1 … {primary}+9 — Switch to a numbered tab"),
            format!("{primary}+O / {primary}+Shift+O — Browse for a root here / in a new tab"),
            format!("{primary}+Shift+R — Open the saved-root selector"),
            "".to_string(),
            "Presets".to_string(),
            format!("{primary}+Shift+P — Open the preset picker"),
            "Type to filter preset names; Up / Down to select; Enter to apply; Esc to close"
                .to_string(),
            "F2 — Edit the selected preset".to_string(),
            format!("{primary}+Enter — Save the preset draft; Esc — Discard it"),
            "Manage named roots... / Manage... — Add, edit, or delete reusable preset roots"
                .to_string(),
            "In Named roots, use Up / Down to select, F2 to edit and Delete to remove".to_string(),
            "Applying a preset updates search state only; it never opens or executes a result"
                .to_string(),
            "Saving an edit updates the catalog only; it does not apply the preset".to_string(),
            "".to_string(),
            "Query syntax".to_string(),
            "TERM — Search the name first, then the visible path".to_string(),
            "name:TERM — Match the file or folder name".to_string(),
            "path:TERM — Match the root-relative path".to_string(),
            "dir:TERM — Match the parent directory path".to_string(),
            "ext:EXT — Match the final extension without the dot (files only)".to_string(),
            "Combine terms: dir:src ext:rs !dir:target".to_string(),
            "Operators after a field: ' exact, ! exclude, ^ start, $ end, | alternatives"
                .to_string(),
            "".to_string(),
        ];
        if emacs_keybindings_enabled {
            lines.extend([
                "Emacs-style shortcuts".to_string(),
                "Ctrl+N / Ctrl+P — Move the current row".to_string(),
                "Ctrl+V / Alt+V — Page down / page up".to_string(),
                "Ctrl+I — Toggle pin".to_string(),
                "Ctrl+J / Ctrl+M — Open or execute".to_string(),
                "Ctrl+G — Clear the query and pinned items".to_string(),
                "Ctrl+R — Search query history".to_string(),
                "Ctrl+A / Ctrl+E — Move to the start / end of the search text".to_string(),
                "Ctrl+B / Ctrl+F — Move one character in the search text".to_string(),
                "Ctrl+H / Ctrl+D — Delete backward / forward".to_string(),
            ]);
            if cfg!(target_os = "macos") {
                lines.push(
                    "Ctrl+W / Ctrl+K — Delete a word / through the end of the line".to_string(),
                );
            } else {
                lines.push("Ctrl+K — Delete through the end of the search text".to_string());
            }
            lines.extend([
                "Ctrl+Y — Restore the most recently deleted search text".to_string(),
                "Ctrl+U — Delete through the start of the search text".to_string(),
            ]);
        } else {
            lines.push("Emacs-style shortcuts are disabled in the runtime config.".to_string());
        }
        lines.extend([
            "".to_string(),
            "Help".to_string(),
            "F1 — Open or close this help".to_string(),
            "Esc — Close this help without changing the current search".to_string(),
        ]);
        lines
    }
}

pub(super) fn render(app: &mut FlistWalkerApp, ctx: &egui::Context) {
    if !app.shell.ui.help_open {
        return;
    }

    let lines = FlistWalkerApp::gui_help_lines(app.shell.runtime.emacs_keybindings_enabled);
    egui::Modal::new(egui::Id::new("gui-help-modal")).show(ctx, |ui| {
        ui.set_min_width(560.0);
        ui.heading("Help");
        ui.label("Keyboard shortcuts and query syntax for the current runtime configuration.");
        ui.add_space(6.0);
        egui::ScrollArea::vertical()
            .max_height(460.0)
            .show(ui, |ui| {
                for line in &lines {
                    if line.is_empty() {
                        ui.add_space(5.0);
                    } else if matches!(
                        line.as_str(),
                        "Search and navigation"
                            | "Selection and actions"
                            | "Tabs and roots"
                            | "Presets"
                            | "Query syntax"
                            | "Emacs-style shortcuts"
                            | "Help"
                    ) {
                        ui.label(egui::RichText::new(line).strong());
                    } else {
                        ui.label(line);
                    }
                }
            });
        ui.add_space(8.0);
        if ui.button("Close").clicked() {
            app.queue_render_command(crate::app::render::RenderCommand::HelpDialog(
                RenderHelpDialogCommand::Close,
            ));
        }
    });
}

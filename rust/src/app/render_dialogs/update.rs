use crate::app::{FlistWalkerApp, UpdateSupport};
use eframe::egui;

pub(in crate::app) fn prompt_command(
    confirm: bool,
    later: bool,
    skip_until_next_version: bool,
) -> Option<crate::app::render::RenderUpdateDialogCommand> {
    if confirm {
        Some(crate::app::render::RenderUpdateDialogCommand::StartInstall)
    } else if later && skip_until_next_version {
        Some(crate::app::render::RenderUpdateDialogCommand::SkipPromptUntilNextVersion)
    } else if later {
        Some(crate::app::render::RenderUpdateDialogCommand::DismissPrompt)
    } else {
        None
    }
}

pub(in crate::app) fn check_failure_command(
    close: bool,
    suppress_future_errors: bool,
) -> Option<crate::app::render::RenderUpdateDialogCommand> {
    if close && suppress_future_errors {
        Some(crate::app::render::RenderUpdateDialogCommand::SuppressCheckFailures)
    } else if close {
        Some(crate::app::render::RenderUpdateDialogCommand::DismissCheckFailure)
    } else {
        None
    }
}

pub(super) fn render_prompt(app: &mut FlistWalkerApp, ctx: &egui::Context) {
    if let Some(prompt) = app.shell.features.update.state.prompt.as_ref().cloned() {
        let mut confirm = false;
        let mut later = false;
        let mut skip_until_next_version = prompt.skip_until_next_version;
        egui::Window::new("Update Available")
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, egui::vec2(0.0, 0.0))
            .show(ctx, |ui| {
                ui.label(format!(
                    "FlistWalker {} is available. Current version is {}.",
                    prompt.candidate.target_version, prompt.candidate.current_version
                ));
                match &prompt.candidate.support {
                    UpdateSupport::Auto => {
                        ui.label(
                            "Download the new release, replace the current binary, and restart?",
                        );
                        if prompt.install_started {
                            ui.label("Downloading update... please wait.");
                        }
                        ui.checkbox(
                            &mut skip_until_next_version,
                            "Don't show again until the next version",
                        );
                        ui.horizontal(|ui| {
                            if ui
                                .add_enabled(
                                    !prompt.install_started,
                                    egui::Button::new("Download and Restart"),
                                )
                                .clicked()
                            {
                                confirm = true;
                            }
                            if ui
                                .add_enabled(!prompt.install_started, egui::Button::new("Later"))
                                .clicked()
                            {
                                later = true;
                            }
                        });
                    }
                    UpdateSupport::ManualOnly { message } => {
                        ui.label(message);
                        ui.label(format!("Release: {}", prompt.candidate.release_url));
                        ui.checkbox(
                            &mut skip_until_next_version,
                            "Don't show again until the next version",
                        );
                        if ui.button("Later").clicked() {
                            later = true;
                        }
                    }
                }
            });

        app.shell
            .features
            .update
            .set_prompt_skip_until_next_version(skip_until_next_version);

        if let Some(command) = prompt_command(confirm, later, skip_until_next_version) {
            app.queue_render_command(crate::app::render::RenderCommand::UpdateDialog(command));
        }
    }
}

pub(super) fn render_check_failure(app: &mut FlistWalkerApp, ctx: &egui::Context) {
    if let Some(failure) = app
        .shell
        .features
        .update
        .state
        .check_failure
        .as_ref()
        .cloned()
    {
        let mut close = false;
        let mut suppress_future_errors = failure.suppress_future_errors;
        egui::Modal::new(egui::Id::new("update-check-failure-modal")).show(ctx, |ui| {
            ui.heading("Update Check Failed");
            ui.label("FlistWalker couldn't check for updates right now.");
            ui.label("You can keep using the app as usual and try again later.");
            ui.add_space(6.0);
            ui.separator();
            ui.label("Details");
            ui.monospace(&failure.error);
            ui.add_space(6.0);
            ui.checkbox(
                &mut suppress_future_errors,
                "Don't show this again for update check errors",
            );
            if ui.button("Close").clicked() {
                close = true;
            }
        });

        app.shell
            .features
            .update
            .set_check_failure_suppress_future_errors(suppress_future_errors);

        if let Some(command) = check_failure_command(close, suppress_future_errors) {
            app.queue_render_command(crate::app::render::RenderCommand::UpdateDialog(command));
        }
    }
}

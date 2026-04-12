use super::*;

// Update reducer command surface. Update-specific state transitions live in
// UpdateManager; this module bridges those commands back into FlistWalkerApp.
#[allow(dead_code)]
pub(super) enum UpdateUiCommand {
    SetNotice(String),
}

#[allow(dead_code)]
pub(super) enum UpdateWorkerCommand {
    Start(UpdateRequest),
}

#[allow(dead_code)]
pub(super) enum UpdateAppCommand {
    MarkUiStateDirty,
    PersistUiStateNow,
    RequestViewportClose,
    AppendWindowTrace {
        event: &'static str,
        details: String,
    },
}

#[allow(dead_code)]
pub(super) enum UpdateCommand {
    Ui(UpdateUiCommand),
    Worker(UpdateWorkerCommand),
    App(UpdateAppCommand),
}

impl FlistWalkerApp {
    fn dispatch_update_commands(
        &mut self,
        ctx: Option<&egui::Context>,
        commands: Vec<UpdateCommand>,
    ) {
        for command in commands {
            match command {
                UpdateCommand::Ui(UpdateUiCommand::SetNotice(notice)) => {
                    self.set_notice(notice);
                }
                UpdateCommand::Worker(UpdateWorkerCommand::Start(req)) => {
                    let is_install = matches!(req.kind, UpdateRequestKind::DownloadAndApply { .. });
                    if self.shell.worker_bus.update.tx.send(req).is_err() {
                        if is_install {
                            let fallback = self.shell.features.update.install_send_failure_commands();
                            self.dispatch_update_commands(ctx, fallback);
                        } else {
                            self.shell.features.update.clear_request();
                        }
                    }
                }
                UpdateCommand::App(UpdateAppCommand::MarkUiStateDirty) => {
                    self.mark_ui_state_dirty();
                }
                UpdateCommand::App(UpdateAppCommand::PersistUiStateNow) => {
                    self.persist_ui_state_now();
                }
                UpdateCommand::App(UpdateAppCommand::RequestViewportClose) => {
                    if let Some(ctx) = ctx {
                        ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                    }
                }
                UpdateCommand::App(UpdateAppCommand::AppendWindowTrace { event, details }) => {
                    Self::append_window_trace(event, &details);
                }
            }
        }
    }

    pub(super) fn request_startup_update_check(&mut self) {
        let commands = self.shell.features
            .update
            .request_startup_check_commands(self_update_disabled());
        self.dispatch_update_commands(None, commands);
    }

    pub(super) fn start_update_install(&mut self) {
        let current_exe = match std::env::current_exe() {
            Ok(path) => path,
            Err(err) => {
                self.set_notice(format!(
                    "Update failed: failed to resolve current executable: {err}"
                ));
                return;
            }
        };
        let commands = match self.shell.features.update.start_install_commands(current_exe) {
            Ok(commands) => commands,
            Err(error) => {
                self.set_notice(error);
                return;
            }
        };
        if commands.is_empty() {
            return;
        }
        self.dispatch_update_commands(None, commands);
    }

    pub(super) fn dismiss_update_prompt(&mut self) {
        self.shell.features.update.dismiss_prompt();
    }

    pub(super) fn dismiss_update_check_failure(&mut self) {
        self.shell.features.update.dismiss_check_failure();
    }

    pub(super) fn suppress_update_check_failures(&mut self) {
        let commands = self.shell.features.update.suppress_check_failures_commands();
        self.dispatch_update_commands(None, commands);
    }

    pub(super) fn skip_update_prompt_until_next_version(&mut self) {
        let commands = self.shell.features
            .update
            .skip_prompt_until_next_version_commands();
        self.dispatch_update_commands(None, commands);
    }

    pub(super) fn poll_update_response(&mut self) {
        while let Ok(response) = self.shell.worker_bus.update.rx.try_recv() {
            let commands = self.shell.features.update.handle_response_commands(response);
            self.dispatch_update_commands(None, commands);
        }
    }
}

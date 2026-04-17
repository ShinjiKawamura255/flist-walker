use super::{
    self_update_disabled, FlistWalkerApp, UpdateRequest, UpdateRequestKind, UpdateResponse,
};
use crate::app::state::{
    UpdateCheckFailureState, UpdateManager, UpdatePromptState, UpdateState,
};
use eframe::egui;
use std::path::PathBuf;

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

impl UpdateManager {
    pub(super) fn from_state(state: UpdateState) -> Self {
        Self { state }
    }

    pub(super) fn request_startup_check_commands(
        &mut self,
        disabled: bool,
    ) -> Vec<UpdateCommand> {
        if disabled {
            self.clear_for_disabled_update();
            return Vec::new();
        }
        let request_id = self.begin_request();
        vec![
            UpdateCommand::Worker(UpdateWorkerCommand::Start(UpdateRequest {
                request_id,
                kind: UpdateRequestKind::Check,
            })),
            UpdateCommand::App(UpdateAppCommand::AppendWindowTrace {
                event: "update_check_requested",
                details: format!("request_id={request_id}"),
            }),
        ]
    }

    pub(super) fn start_install_commands(
        &mut self,
        current_exe: PathBuf,
    ) -> Result<Vec<UpdateCommand>, String> {
        let Some(prompt) = self.state.prompt.as_ref() else {
            return Ok(Vec::new());
        };
        if prompt.install_started {
            return Ok(Vec::new());
        }
        let candidate = prompt.candidate.clone();
        if let Some(prompt) = self.state.prompt.as_mut() {
            prompt.install_started = true;
        }
        let request_id = self.begin_request();
        Ok(vec![
            UpdateCommand::Worker(UpdateWorkerCommand::Start(UpdateRequest {
                request_id,
                kind: UpdateRequestKind::DownloadAndApply {
                    candidate: Box::new(candidate.clone()),
                    current_exe,
                },
            })),
            UpdateCommand::Ui(UpdateUiCommand::SetNotice(format!(
                "Downloading update {}...",
                candidate.target_version
            ))),
            UpdateCommand::App(UpdateAppCommand::AppendWindowTrace {
                event: "update_install_requested",
                details: format!(
                    "request_id={request_id} target_version={}",
                    candidate.target_version
                ),
            }),
        ])
    }

    pub(super) fn install_send_failure_commands(&mut self) -> Vec<UpdateCommand> {
        self.clear_request();
        if let Some(prompt) = self.state.prompt.as_mut() {
            prompt.install_started = false;
        }
        vec![UpdateCommand::Ui(UpdateUiCommand::SetNotice(
            "Update worker is unavailable".to_string(),
        ))]
    }

    pub(super) fn dismiss_prompt(&mut self) {
        self.state.prompt = None;
    }

    pub(super) fn dismiss_check_failure(&mut self) {
        self.state.check_failure = None;
    }

    pub(super) fn set_prompt_skip_until_next_version(&mut self, skip: bool) {
        if let Some(prompt) = self.state.prompt.as_mut() {
            prompt.skip_until_next_version = skip;
        }
    }

    pub(super) fn set_check_failure_suppress_future_errors(&mut self, suppress: bool) {
        if let Some(failure) = self.state.check_failure.as_mut() {
            failure.suppress_future_errors = suppress;
        }
    }

    pub(super) fn suppress_check_failures_commands(&mut self) -> Vec<UpdateCommand> {
        self.state.suppress_check_failure_dialog = true;
        self.state.check_failure = None;
        vec![
            UpdateCommand::App(UpdateAppCommand::MarkUiStateDirty),
            UpdateCommand::App(UpdateAppCommand::PersistUiStateNow),
            UpdateCommand::Ui(UpdateUiCommand::SetNotice(
                "Startup update check errors will be hidden".to_string(),
            )),
        ]
    }

    pub(super) fn skip_prompt_until_next_version_commands(&mut self) -> Vec<UpdateCommand> {
        let Some(target_version) = self
            .state
            .prompt
            .as_ref()
            .map(|prompt| prompt.candidate.target_version.clone())
        else {
            return Vec::new();
        };
        self.state.skipped_target_version = Some(target_version.clone());
        self.state.prompt = None;
        vec![
            UpdateCommand::App(UpdateAppCommand::MarkUiStateDirty),
            UpdateCommand::App(UpdateAppCommand::PersistUiStateNow),
            UpdateCommand::Ui(UpdateUiCommand::SetNotice(format!(
                "Update {} hidden until a newer version is available",
                target_version
            ))),
        ]
    }

    pub(super) fn handle_response_commands(&mut self, response: UpdateResponse) -> Vec<UpdateCommand> {
        match response {
            UpdateResponse::UpToDate { request_id } => {
                if !self.settle_response(request_id) {
                    return Vec::new();
                }
                vec![UpdateCommand::App(UpdateAppCommand::AppendWindowTrace {
                    event: "update_up_to_date",
                    details: format!("request_id={request_id}"),
                })]
            }
            UpdateResponse::CheckFailed { request_id, error } => {
                if !self.settle_response(request_id) {
                    return Vec::new();
                }
                let commands = vec![UpdateCommand::App(UpdateAppCommand::AppendWindowTrace {
                    event: "update_check_failed",
                    details: format!("request_id={request_id} error={error}"),
                })];
                if !self.state.suppress_check_failure_dialog
                    || super::forced_update_check_failure_message().is_some()
                {
                    self.state.check_failure = Some(UpdateCheckFailureState {
                        error,
                        suppress_future_errors: false,
                    });
                }
                commands
            }
            UpdateResponse::Available {
                request_id,
                candidate,
            } => {
                if !self.settle_response(request_id) {
                    return Vec::new();
                }
                let target_version = candidate.target_version.clone();
                if !super::should_skip_update_prompt(
                    &target_version,
                    self.state.skipped_target_version.as_deref(),
                ) {
                    self.state.prompt = Some(UpdatePromptState {
                        candidate: *candidate,
                        skip_until_next_version: false,
                        install_started: false,
                    });
                }
                vec![UpdateCommand::App(UpdateAppCommand::AppendWindowTrace {
                    event: "update_available",
                    details: format!("request_id={request_id} target_version={target_version}"),
                })]
            }
            UpdateResponse::ApplyStarted {
                request_id,
                target_version,
            } => {
                if !self.settle_response(request_id) {
                    return Vec::new();
                }
                self.state.prompt = None;
                self.state.close_requested_for_install = true;
                vec![
                    UpdateCommand::Ui(UpdateUiCommand::SetNotice(format!(
                        "Restarting to apply update {}...",
                        target_version
                    ))),
                    UpdateCommand::App(UpdateAppCommand::RequestViewportClose),
                    UpdateCommand::App(UpdateAppCommand::AppendWindowTrace {
                        event: "update_apply_started",
                        details: format!(
                            "request_id={request_id} target_version={target_version}"
                        ),
                    }),
                ]
            }
            UpdateResponse::Failed { request_id, error } => {
                if !self.settle_response(request_id) {
                    return Vec::new();
                }
                let details_error = error.clone();
                if let Some(prompt) = self.state.prompt.as_mut() {
                    prompt.install_started = false;
                }
                vec![
                    UpdateCommand::Ui(UpdateUiCommand::SetNotice(error)),
                    UpdateCommand::App(UpdateAppCommand::AppendWindowTrace {
                        event: "update_failed",
                        details: format!("request_id={request_id} error={details_error}"),
                    }),
                ]
            }
        }
    }

    pub(super) fn clear_request(&mut self) {
        self.state.pending_request_id = None;
        self.state.in_progress = false;
    }

    pub(super) fn clear_for_disabled_update(&mut self) {
        self.clear_request();
    }

    pub(super) fn begin_request(&mut self) -> u64 {
        let request_id = self.state.next_request_id;
        self.state.next_request_id = self.state.next_request_id.saturating_add(1);
        self.state.pending_request_id = Some(request_id);
        self.state.in_progress = true;
        request_id
    }

    pub(super) fn settle_response(&mut self, request_id: u64) -> bool {
        if self.state.pending_request_id != Some(request_id) {
            return false;
        }
        self.clear_request();
        true
    }
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
                            let fallback =
                                self.shell.features.update.install_send_failure_commands();
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
        let commands = self
            .shell
            .features
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
        let commands = match self
            .shell
            .features
            .update
            .start_install_commands(current_exe)
        {
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
        let commands = self
            .shell
            .features
            .update
            .suppress_check_failures_commands();
        self.dispatch_update_commands(None, commands);
    }

    pub(super) fn skip_update_prompt_until_next_version(&mut self) {
        let commands = self
            .shell
            .features
            .update
            .skip_prompt_until_next_version_commands();
        self.dispatch_update_commands(None, commands);
    }

    pub(super) fn poll_update_response(&mut self) {
        while let Ok(response) = self.shell.worker_bus.update.rx.try_recv() {
            let commands = self
                .shell
                .features
                .update
                .handle_response_commands(response);
            self.dispatch_update_commands(None, commands);
        }
    }
}

use super::*;

// Phase 1 scaffolding for the Update reducer split. Later phases will move
// update-specific state transitions behind an UpdateManager that emits these
// commands instead of mutating FlistWalkerApp directly from every branch.
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
}

#[allow(dead_code)]
pub(super) enum UpdateCommand {
    Ui(UpdateUiCommand),
    Worker(UpdateWorkerCommand),
    App(UpdateAppCommand),
}

impl FlistWalkerApp {
    pub(super) fn request_startup_update_check(&mut self) {
        if self_update_disabled() {
            self.update_state.pending_request_id = None;
            self.update_state.in_progress = false;
            return;
        }
        let request_id = self.update_state.next_request_id;
        self.update_state.next_request_id = self.update_state.next_request_id.saturating_add(1);
        self.update_state.pending_request_id = Some(request_id);
        self.update_state.in_progress = true;
        if self
            .worker_bus
            .update
            .tx
            .send(UpdateRequest {
                request_id,
                kind: UpdateRequestKind::Check,
            })
            .is_err()
        {
            self.update_state.pending_request_id = None;
            self.update_state.in_progress = false;
        }
    }

    pub(super) fn start_update_install(&mut self) {
        let Some(prompt) = self.update_state.prompt.as_ref() else {
            return;
        };
        if prompt.install_started {
            return;
        }
        let candidate = prompt.candidate.clone();
        let current_exe = match std::env::current_exe() {
            Ok(path) => path,
            Err(err) => {
                self.set_notice(format!(
                    "Update failed: failed to resolve current executable: {err}"
                ));
                return;
            }
        };
        if let Some(prompt) = self.update_state.prompt.as_mut() {
            prompt.install_started = true;
        }
        let request_id = self.update_state.next_request_id;
        self.update_state.next_request_id = self.update_state.next_request_id.saturating_add(1);
        self.update_state.pending_request_id = Some(request_id);
        self.update_state.in_progress = true;
        if self
            .worker_bus
            .update
            .tx
            .send(UpdateRequest {
                request_id,
                kind: UpdateRequestKind::DownloadAndApply {
                    candidate: Box::new(candidate.clone()),
                    current_exe,
                },
            })
            .is_err()
        {
            self.update_state.pending_request_id = None;
            self.update_state.in_progress = false;
            if let Some(prompt) = self.update_state.prompt.as_mut() {
                prompt.install_started = false;
            }
            self.set_notice("Update worker is unavailable");
            return;
        }
        self.set_notice(format!(
            "Downloading update {}...",
            candidate.target_version
        ));
    }

    pub(super) fn dismiss_update_prompt(&mut self) {
        self.update_state.prompt = None;
    }

    pub(super) fn dismiss_update_check_failure(&mut self) {
        self.update_state.check_failure = None;
    }

    pub(super) fn suppress_update_check_failures(&mut self) {
        self.update_state.suppress_check_failure_dialog = true;
        self.update_state.check_failure = None;
        self.mark_ui_state_dirty();
        self.persist_ui_state_now();
        self.set_notice("Startup update check errors will be hidden");
    }

    pub(super) fn skip_update_prompt_until_next_version(&mut self) {
        let Some(target_version) = self
            .update_state
            .prompt
            .as_ref()
            .map(|prompt| prompt.candidate.target_version.clone())
        else {
            return;
        };
        self.update_state.skipped_target_version = Some(target_version.clone());
        self.mark_ui_state_dirty();
        self.persist_ui_state_now();
        self.update_state.prompt = None;
        self.set_notice(format!(
            "Update {} hidden until a newer version is available",
            target_version
        ));
    }

    pub(super) fn update_prompt_is_suppressed(&self, candidate: &UpdateCandidate) -> bool {
        should_skip_update_prompt(
            &candidate.target_version,
            self.update_state.skipped_target_version.as_deref(),
        )
    }

    pub(super) fn poll_update_response(&mut self) {
        while let Ok(response) = self.worker_bus.update.rx.try_recv() {
            let Some(pending) = self.update_state.pending_request_id else {
                continue;
            };
            match response {
                UpdateResponse::UpToDate { request_id } => {
                    if request_id != pending {
                        continue;
                    }
                    self.update_state.pending_request_id = None;
                    self.update_state.in_progress = false;
                }
                UpdateResponse::CheckFailed { request_id, error } => {
                    if request_id != pending {
                        continue;
                    }
                    self.update_state.pending_request_id = None;
                    self.update_state.in_progress = false;
                    Self::append_window_trace("update_check_failed", &error);
                    if !self.update_state.suppress_check_failure_dialog
                        || forced_update_check_failure_message().is_some()
                    {
                        self.update_state.check_failure = Some(UpdateCheckFailureState {
                            error,
                            suppress_future_errors: false,
                        });
                    }
                }
                UpdateResponse::Available {
                    request_id,
                    candidate,
                } => {
                    if request_id != pending {
                        continue;
                    }
                    self.update_state.pending_request_id = None;
                    self.update_state.in_progress = false;
                    if !self.update_prompt_is_suppressed(&candidate) {
                        self.update_state.prompt = Some(UpdatePromptState {
                            candidate: *candidate,
                            skip_until_next_version: false,
                            install_started: false,
                        });
                    }
                }
                UpdateResponse::ApplyStarted {
                    request_id,
                    target_version,
                } => {
                    if request_id != pending {
                        continue;
                    }
                    self.update_state.pending_request_id = None;
                    self.update_state.in_progress = false;
                    self.update_state.prompt = None;
                    self.set_notice(format!("Restarting to apply update {}...", target_version));
                    self.update_state.close_requested_for_install = true;
                }
                UpdateResponse::Failed { request_id, error } => {
                    if request_id != pending {
                        continue;
                    }
                    self.update_state.pending_request_id = None;
                    self.update_state.in_progress = false;
                    if let Some(prompt) = self.update_state.prompt.as_mut() {
                        prompt.install_started = false;
                    }
                    self.set_notice(error);
                }
            }
        }
    }
}

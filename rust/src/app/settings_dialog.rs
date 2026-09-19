use super::worker::config_settings::{ConfigSettingsCompletion, ConfigSettingsRequest};
use super::FlistWalkerApp;
#[cfg(test)]
use crate::runtime_config::runtime_config_file_path_in;
use crate::runtime_config::{runtime_config_file_path, EditableSettings, EditableSettingsSnapshot};
use std::path::PathBuf;

pub(super) enum SettingsView {
    Closed,
    Loading,
    Reloading {
        baseline: EditableSettingsSnapshot,
        draft: EditableSettings,
        limit_text: String,
    },
    Editing {
        baseline: EditableSettingsSnapshot,
        draft: EditableSettings,
        limit_text: String,
        error: Option<String>,
        confirm_reload: bool,
    },
    Saving {
        baseline: EditableSettingsSnapshot,
        draft: EditableSettings,
        limit_text: String,
    },
    Failed(String),
}

pub(super) struct SettingsDialogState {
    pub(super) generation: u64,
    pub(super) view: SettingsView,
}

impl Default for SettingsDialogState {
    fn default() -> Self {
        Self {
            generation: 0,
            view: SettingsView::Closed,
        }
    }
}

impl SettingsDialogState {
    pub(super) fn is_open(&self) -> bool {
        !matches!(self.view, SettingsView::Closed)
    }

    pub(super) fn is_busy(&self) -> bool {
        matches!(
            self.view,
            SettingsView::Loading | SettingsView::Reloading { .. } | SettingsView::Saving { .. }
        )
    }
}

impl FlistWalkerApp {
    fn settings_config_path(&self) -> Option<PathBuf> {
        #[cfg(test)]
        if let Some(paths) = &self.test_settings_paths {
            return paths.ui_state.parent().map(runtime_config_file_path_in);
        }
        runtime_config_file_path()
    }

    pub(super) fn open_settings_dialog(&mut self) {
        if self.settings_dialog.is_open() || self.shell.worker_bus.config_settings.in_progress() {
            return;
        }
        self.settings_dialog.generation = self.settings_dialog.generation.saturating_add(1);
        self.start_settings_load();
    }

    fn start_settings_load(&mut self) {
        let Some(path) = self.settings_config_path() else {
            self.settings_dialog.view = SettingsView::Failed("Settings path is unavailable".into());
            return;
        };
        match self.shell.worker_bus.config_settings.start(
            self.settings_dialog.generation,
            ConfigSettingsRequest::Load(path),
        ) {
            Ok(()) => self.settings_dialog.view = SettingsView::Loading,
            Err(error) => self.settings_dialog.view = SettingsView::Failed(error.into()),
        }
    }

    pub(super) fn retry_settings_load(&mut self) {
        if matches!(self.settings_dialog.view, SettingsView::Failed(_)) {
            self.settings_dialog.generation = self.settings_dialog.generation.saturating_add(1);
            self.start_settings_load();
        }
    }

    pub(super) fn request_settings_reload(&mut self) {
        let SettingsView::Editing {
            baseline,
            draft,
            limit_text,
            confirm_reload,
            ..
        } = &mut self.settings_dialog.view
        else {
            return;
        };
        let limit_text_changed =
            limit_text.trim() != baseline.values.walker_max_entries.to_string();
        if (draft != &baseline.values || limit_text_changed) && !*confirm_reload {
            *confirm_reload = true;
            return;
        }
        let baseline = baseline.clone();
        let draft = draft.clone();
        let limit_text = limit_text.clone();
        let Some(path) = self.settings_config_path() else {
            if let SettingsView::Editing { error, .. } = &mut self.settings_dialog.view {
                *error = Some("Settings path is unavailable".into());
            }
            return;
        };
        match self.shell.worker_bus.config_settings.start(
            self.settings_dialog.generation,
            ConfigSettingsRequest::Load(path),
        ) {
            Ok(()) => {
                self.settings_dialog.view = SettingsView::Reloading {
                    baseline,
                    draft,
                    limit_text,
                }
            }
            Err(message) => {
                if let SettingsView::Editing { error, .. } = &mut self.settings_dialog.view {
                    *error = Some(message.into());
                }
            }
        }
    }

    pub(super) fn request_settings_save(&mut self) {
        let SettingsView::Editing {
            baseline,
            draft,
            limit_text,
            error,
            ..
        } = &mut self.settings_dialog.view
        else {
            return;
        };
        let Ok(limit) = limit_text.trim().parse::<usize>() else {
            *error = Some("Walker entry limit must be a positive integer".into());
            return;
        };
        if limit == 0 {
            *error = Some("Walker entry limit must be at least 1".into());
            return;
        }
        draft.walker_max_entries = limit;
        let baseline = baseline.clone();
        let draft = draft.clone();
        let limit_text = limit_text.clone();
        let Some(path) = self.settings_config_path() else {
            if let SettingsView::Editing { error, .. } = &mut self.settings_dialog.view {
                *error = Some("Settings path is unavailable".into());
            }
            return;
        };
        match self.shell.worker_bus.config_settings.start(
            self.settings_dialog.generation,
            ConfigSettingsRequest::Save {
                path,
                baseline: baseline.clone(),
                draft: draft.clone(),
            },
        ) {
            Ok(()) => {
                self.settings_dialog.view = SettingsView::Saving {
                    baseline,
                    draft,
                    limit_text,
                }
            }
            Err(message) => {
                if let SettingsView::Editing { error, .. } = &mut self.settings_dialog.view {
                    *error = Some(message.into());
                }
            }
        }
    }

    pub(super) fn close_settings_dialog(&mut self) {
        if !matches!(self.settings_dialog.view, SettingsView::Saving { .. }) {
            self.settings_dialog.generation = self.settings_dialog.generation.saturating_add(1);
            self.settings_dialog.view = SettingsView::Closed;
        }
    }

    pub(super) fn poll_config_settings_response(&mut self) {
        let Some((generation, response)) = self.shell.worker_bus.config_settings.poll() else {
            return;
        };
        if generation != self.settings_dialog.generation {
            return;
        }
        match response {
            ConfigSettingsCompletion::Loaded(Ok(baseline)) => {
                let draft = baseline.values.clone();
                self.settings_dialog.view = SettingsView::Editing {
                    limit_text: draft.walker_max_entries.to_string(),
                    baseline,
                    draft,
                    error: None,
                    confirm_reload: false,
                };
            }
            ConfigSettingsCompletion::Loaded(Err(error)) => {
                let view = std::mem::replace(&mut self.settings_dialog.view, SettingsView::Closed);
                self.settings_dialog.view = match view {
                    SettingsView::Reloading {
                        baseline,
                        draft,
                        limit_text,
                    } => SettingsView::Editing {
                        baseline,
                        draft,
                        limit_text,
                        error: Some(error),
                        confirm_reload: false,
                    },
                    _ => SettingsView::Failed(error),
                };
            }
            ConfigSettingsCompletion::Saved(Ok(_)) => {
                self.settings_dialog.view = SettingsView::Closed;
                self.set_notice("Settings saved. Changes take effect on next launch");
            }
            ConfigSettingsCompletion::Saved(Err(error)) => {
                let view = std::mem::replace(&mut self.settings_dialog.view, SettingsView::Closed);
                self.settings_dialog.view = match view {
                    SettingsView::Saving {
                        baseline,
                        draft,
                        limit_text,
                    } => SettingsView::Editing {
                        baseline,
                        draft,
                        limit_text,
                        error: Some(error),
                        confirm_reload: false,
                    },
                    _ => SettingsView::Failed(error),
                };
            }
        }
    }
}

use super::{FlistWalkerApp, UpdateSupport};
use serde::Serialize;

#[allow(dead_code)]
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(super) struct DialogSnapshot {
    pub(super) title: String,
    pub(super) lines: Vec<String>,
    pub(super) buttons: Vec<String>,
}

#[allow(dead_code)]
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(super) struct GuiSurfaceSnapshot {
    pub(super) history_search_active: bool,
    pub(super) show_preview: bool,
    pub(super) preview_panel_width: u32,
    pub(super) top_actions: Vec<String>,
    pub(super) status_line: String,
    pub(super) filelist_dialogs: Vec<DialogSnapshot>,
    pub(super) update_dialogs: Vec<DialogSnapshot>,
}

#[allow(dead_code)]
fn preview_width_px(width: f32) -> u32 {
    width.round().max(0.0) as u32
}

#[allow(dead_code)]
pub(super) fn gui_surface_snapshot(app: &FlistWalkerApp) -> GuiSurfaceSnapshot {
    let mut filelist_dialogs = Vec::new();
    if let Some(pending) = app
        .shell
        .features
        .filelist
        .workflow
        .pending_confirmation
        .as_ref()
    {
        filelist_dialogs.push(DialogSnapshot {
            title: "Overwrite FileList?".to_string(),
            lines: vec![format!(
                "{} already exists. Overwrite it?",
                pending.existing_path.display()
            )],
            buttons: vec!["Overwrite".to_string(), "Cancel".to_string()],
        });
    }
    if app
        .shell
        .features
        .filelist
        .workflow
        .pending_ancestor_confirmation
        .is_some()
    {
        filelist_dialogs.push(DialogSnapshot {
            title: "Update Ancestor FileLists?".to_string(),
            lines: vec![
                "親ディレクトリ直下の既存 FileList にも参照を追記します。".to_string(),
                "Continue は祖先も更新し、Current Root Only は現在 root の FileList だけを作成します。"
                    .to_string(),
            ],
            buttons: vec![
                "Continue".to_string(),
                "Current Root Only".to_string(),
                "Cancel".to_string(),
            ],
        });
    }
    if app
        .shell
        .features
        .filelist
        .workflow
        .pending_use_walker_confirmation
        .is_some()
    {
        let [line1, line2] = FlistWalkerApp::filelist_use_walker_dialog_lines();
        filelist_dialogs.push(DialogSnapshot {
            title: "Create File List?".to_string(),
            lines: vec![line1.to_string(), line2.to_string()],
            buttons: vec!["Continue".to_string(), "Cancel".to_string()],
        });
    }

    let mut update_dialogs = Vec::new();
    if let Some(prompt) = app.shell.features.update.state.prompt.as_ref() {
        let (title, lines, buttons) = match &prompt.candidate.support {
            UpdateSupport::Auto => (
                "Update Available".to_string(),
                vec![
                    format!(
                        "FlistWalker {} is available. Current version is {}.",
                        prompt.candidate.target_version, prompt.candidate.current_version
                    ),
                    "Download the new release, replace the current binary, and restart?"
                        .to_string(),
                ],
                vec!["Download and Restart".to_string(), "Later".to_string()],
            ),
            UpdateSupport::ManualOnly { message } => (
                "Update Available".to_string(),
                vec![
                    format!(
                        "FlistWalker {} is available. Current version is {}.",
                        prompt.candidate.target_version, prompt.candidate.current_version
                    ),
                    message.clone(),
                    format!("Release: {}", prompt.candidate.release_url),
                ],
                vec!["Later".to_string()],
            ),
        };
        update_dialogs.push(DialogSnapshot {
            title,
            lines,
            buttons,
        });
    }
    if let Some(failure) = app.shell.features.update.state.check_failure.as_ref() {
        update_dialogs.push(DialogSnapshot {
            title: "Update Check Failed".to_string(),
            lines: vec![
                "FlistWalker couldn't check for updates right now.".to_string(),
                "You can keep using the app as usual and try again later.".to_string(),
                "Details".to_string(),
                failure.error.clone(),
            ],
            buttons: vec!["Close".to_string()],
        });
    }

    GuiSurfaceSnapshot {
        history_search_active: app.shell.runtime.query_state.history_search_active,
        show_preview: app.shell.ui.show_preview(),
        preview_panel_width: preview_width_px(app.shell.ui.preview_panel_width()),
        top_actions: app
            .top_action_labels()
            .into_iter()
            .map(str::to_string)
            .collect(),
        status_line: app.shell.runtime.status_line.clone(),
        filelist_dialogs,
        update_dialogs,
    }
}

use super::{FlistWalkerApp, UpdateSupport};
use crate::ui_model::normalize_path_for_display;
use serde::Serialize;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(super) struct DialogSnapshot {
    pub(super) title: String,
    pub(super) lines: Vec<String>,
    pub(super) buttons: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(super) struct GuiSurfaceSnapshot {
    pub(super) root: String,
    pub(super) query: String,
    pub(super) use_filelist: bool,
    pub(super) use_regex: bool,
    pub(super) ignore_case: bool,
    pub(super) ignore_list_enabled: bool,
    pub(super) include_files: bool,
    pub(super) include_dirs: bool,
    pub(super) max_depth: String,
    pub(super) result_sort_mode: String,
    pub(super) result_sort_scope: String,
    pub(super) result_count: usize,
    pub(super) total_match_count: usize,
    pub(super) current_result: Option<String>,
    pub(super) pinned_count: usize,
    pub(super) tab_count: usize,
    pub(super) active_tab: usize,
    pub(super) history_search_active: bool,
    pub(super) show_preview: bool,
    pub(super) preview_panel_width: u32,
    pub(super) top_actions: Vec<String>,
    pub(super) status_line: String,
    pub(super) help_dialogs: Vec<DialogSnapshot>,
    pub(super) preset_picker_dialogs: Vec<DialogSnapshot>,
    pub(super) filelist_dialogs: Vec<DialogSnapshot>,
    pub(super) update_dialogs: Vec<DialogSnapshot>,
}

fn preview_width_px(width: f32) -> u32 {
    width.round().max(0.0) as u32
}

pub(super) fn gui_surface_snapshot(app: &FlistWalkerApp) -> GuiSurfaceSnapshot {
    let help_dialogs = if app.shell.ui.help_open {
        vec![DialogSnapshot {
            title: "Help".to_string(),
            lines: FlistWalkerApp::gui_help_lines(
                app.shell.runtime.emacs_keybindings_enabled,
                app.shell.runtime.ctrl_w_deletes_word_in_query,
            ),
            buttons: vec!["Close".to_string()],
        }]
    } else {
        Vec::new()
    };
    let preset_picker_dialogs = if app.shell.features.presets.picker.open {
        if app.shell.features.presets.picker.named_roots.open {
            let manager = &app.shell.features.presets.picker.named_roots;
            if manager.editor.open {
                vec![DialogSnapshot {
                    title: if manager.editor.original_name.is_some() {
                        "Edit named root".to_string()
                    } else {
                        "Add named root".to_string()
                    },
                    lines: vec![
                        format!("Name: {}", manager.editor.name),
                        format!("Path: {}", manager.editor.path),
                    ],
                    buttons: vec![
                        "Browse...".to_string(),
                        "Use current root".to_string(),
                        "Save".to_string(),
                        "Cancel".to_string(),
                    ],
                }]
            } else if manager.confirm_delete {
                let name = manager
                    .selected_index
                    .and_then(|index| app.shell.features.presets.catalog.named_roots.get(index))
                    .map(|root| root.name.clone())
                    .unwrap_or_default();
                vec![DialogSnapshot {
                    title: "Delete named root?".to_string(),
                    lines: vec![name],
                    buttons: vec!["Delete root".to_string(), "Cancel".to_string()],
                }]
            } else {
                let lines = app
                    .shell
                    .features
                    .presets
                    .catalog
                    .named_roots
                    .iter()
                    .map(|root| {
                        format!("{} — {}", root.name, normalize_path_for_display(&root.path))
                    })
                    .collect();
                vec![DialogSnapshot {
                    title: "Named roots".to_string(),
                    lines,
                    buttons: vec![
                        "Add".to_string(),
                        "Edit".to_string(),
                        "Delete".to_string(),
                        "Back".to_string(),
                    ],
                }]
            }
        } else if app.shell.features.presets.picker.editor.open {
            let editor = &app.shell.features.presets.picker.editor;
            vec![DialogSnapshot {
                title: if editor.original_name.is_empty() {
                    "Add preset".to_string()
                } else {
                    "Edit preset".to_string()
                },
                lines: vec![
                    format!("Name: {}", editor.name),
                    format!("Query: {}", editor.query),
                    editor.max_depth.value().map_or_else(
                        || "Max depth: All".to_string(),
                        |depth| format!("Max depth: {depth}"),
                    ),
                ],
                buttons: vec![
                    "Browse...".to_string(),
                    "Save".to_string(),
                    "Cancel".to_string(),
                ],
            }]
        } else if app.shell.features.presets.picker.confirm_delete {
            let name = app
                .shell
                .features
                .presets
                .picker
                .selected_match
                .and_then(|match_index| {
                    app.shell
                        .features
                        .presets
                        .picker
                        .matched_catalog_indices
                        .get(match_index)
                })
                .and_then(|catalog_index| {
                    app.shell
                        .features
                        .presets
                        .catalog
                        .presets
                        .get(*catalog_index)
                })
                .map(|preset| preset.name.clone())
                .unwrap_or_default();
            vec![DialogSnapshot {
                title: "Delete preset?".to_string(),
                lines: vec![name],
                buttons: vec!["Delete preset".to_string(), "Cancel".to_string()],
            }]
        } else {
            let lines = app
                .shell
                .features
                .presets
                .picker
                .matched_catalog_indices
                .iter()
                .filter_map(|index| {
                    app.shell
                        .features
                        .presets
                        .catalog
                        .presets
                        .get(*index)
                        .map(|preset| preset.name.clone())
                })
                .collect();
            vec![DialogSnapshot {
                title: "Presets".to_string(),
                lines,
                buttons: vec![
                    "Manage named roots...".to_string(),
                    "Add".to_string(),
                    "Edit".to_string(),
                    "Delete".to_string(),
                    "Close".to_string(),
                ],
            }]
        }
    } else {
        Vec::new()
    };
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
        root: app.shell.runtime.root.display().to_string(),
        query: app.shell.runtime.query_state.query.clone(),
        use_filelist: app.shell.runtime.use_filelist,
        use_regex: app.shell.runtime.use_regex,
        ignore_case: app.shell.runtime.ignore_case,
        ignore_list_enabled: app.shell.ui.ignore_list_enabled(),
        include_files: app.shell.runtime.include_files,
        include_dirs: app.shell.runtime.include_dirs,
        max_depth: app.shell.runtime.max_depth.value().map_or_else(
            || "Depth: All".to_string(),
            |depth| format!("Depth: ≤ {depth}"),
        ),
        result_sort_mode: app.shell.runtime.result_sort_mode.label().to_string(),
        result_sort_scope: app.shell.runtime.result_sort_scope.label().to_string(),
        result_count: app.shell.runtime.results.len(),
        total_match_count: app.shell.runtime.total_match_count,
        current_result: app
            .shell
            .runtime
            .current_row
            .and_then(|row| app.shell.runtime.results.get(row))
            .map(|(path, _)| path.display().to_string()),
        pinned_count: app.shell.runtime.pinned_paths.len(),
        tab_count: app.shell.tabs.len(),
        active_tab: app.shell.tabs.active_tab_index(),
        history_search_active: app.shell.runtime.query_state.history_search_active,
        show_preview: app.shell.ui.show_preview(),
        preview_panel_width: preview_width_px(app.shell.ui.preview_panel_width()),
        top_actions: app
            .top_action_labels()
            .into_iter()
            .map(str::to_string)
            .collect(),
        status_line: app.shell.runtime.status_line.clone(),
        help_dialogs,
        preset_picker_dialogs,
        filelist_dialogs,
        update_dialogs,
    }
}

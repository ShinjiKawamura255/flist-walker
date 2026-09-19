use super::{egui, FlistWalkerApp, PathBuf};
use std::fs;
use std::time::{SystemTime, UNIX_EPOCH};

pub(crate) fn test_root(name: &str) -> PathBuf {
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("clock")
        .as_nanos();
    std::env::temp_dir().join(format!("fff-rs-app-{name}-{nonce}"))
}

pub(crate) struct TestSettingsScope {
    base: PathBuf,
}

impl TestSettingsScope {
    pub(crate) fn new(name: &str) -> Self {
        let base = test_root(name);
        fs::create_dir_all(&base).expect("create test settings dir");
        Self { base }
    }

    pub(crate) fn app(&self, root: PathBuf, limit: usize, query: String) -> FlistWalkerApp {
        FlistWalkerApp::build_new_with_test_settings(root, limit, query, &self.base)
    }

    pub(crate) fn saved_roots_path(&self) -> PathBuf {
        FlistWalkerApp::saved_roots_file_path_in(&self.base)
    }

    pub(crate) fn runtime_config_path(&self) -> PathBuf {
        crate::runtime_config::runtime_config_file_path_in(&self.base)
    }
}

impl Drop for TestSettingsScope {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.base);
    }
}

pub(crate) fn test_settings_scope(name: &str) -> TestSettingsScope {
    TestSettingsScope::new(name)
}

pub(crate) fn entries_count_from_status(status_line: &str) -> usize {
    status_line
        .split("Entries: ")
        .nth(1)
        .and_then(|rest| rest.split(" | ").next())
        .and_then(|n| n.parse::<usize>().ok())
        .unwrap_or(0)
}

pub(crate) fn run_shortcuts_frame(
    app: &mut FlistWalkerApp,
    query_focused: bool,
    events: Vec<egui::Event>,
) {
    let modifiers = events
        .iter()
        .find_map(|event| {
            if let egui::Event::Key {
                pressed: true,
                modifiers: event_modifiers,
                ..
            } = event
            {
                Some(*event_modifiers)
            } else {
                None
            }
        })
        .unwrap_or(egui::Modifiers::NONE);
    run_shortcuts_frame_with_modifiers(app, query_focused, modifiers, events);
}

pub(crate) fn run_shortcuts_frame_with_modifiers(
    app: &mut FlistWalkerApp,
    query_focused: bool,
    modifiers: egui::Modifiers,
    events: Vec<egui::Event>,
) {
    let ctx = egui::Context::default();
    ctx.begin_pass(egui::RawInput {
        modifiers,
        events,
        ..Default::default()
    });
    if query_focused {
        ctx.memory_mut(|m| m.request_focus(app.shell.ui.query_input_id));
    }
    app.handle_shortcuts(&ctx);
    app.run_deferred_shortcuts(&ctx);
    let _ = ctx.end_pass();
}

pub(crate) fn gui_shortcut_modifiers(shift: bool) -> egui::Modifiers {
    #[cfg(target_os = "macos")]
    {
        egui::Modifiers {
            mac_cmd: true,
            shift,
            ..Default::default()
        }
    }
    #[cfg(not(target_os = "macos"))]
    {
        egui::Modifiers {
            ctrl: true,
            shift,
            ..Default::default()
        }
    }
}

pub(crate) fn tab_switch_shortcut_modifiers(shift: bool) -> egui::Modifiers {
    egui::Modifiers {
        ctrl: true,
        shift,
        ..Default::default()
    }
}

pub(crate) fn emacs_shortcut_modifiers(shift: bool) -> egui::Modifiers {
    egui::Modifiers {
        ctrl: true,
        shift,
        ..Default::default()
    }
}

pub(crate) fn is_action_notice(text: &str) -> bool {
    text.starts_with("Action: ") || text.starts_with("Action failed:")
}

pub(crate) fn commit_query_history_for_test(app: &mut FlistWalkerApp) {
    app.commit_query_history_if_needed(true);
}

pub(crate) fn reset_index_request_state_for_test(app: &mut FlistWalkerApp) {
    app.shell.indexing.pending_request_id = None;
    app.shell.indexing.in_progress = false;
    app.shell.indexing.request_tabs.clear();
    app.shell.indexing.pending_queue.clear();
    app.shell.indexing.inflight_requests.clear();
    app.shell.indexing.superseded_request_ids.clear();
    if let Ok(mut latest) = app.shell.indexing.latest_request_ids.lock() {
        latest.clear();
    }
}

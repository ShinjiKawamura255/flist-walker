use super::{egui, FlistWalkerApp, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

pub(crate) fn test_root(name: &str) -> PathBuf {
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("clock")
        .as_nanos();
    std::env::temp_dir().join(format!("fff-rs-app-{name}-{nonce}"))
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
    let mut modifiers = egui::Modifiers::NONE;
    for event in &events {
        if let egui::Event::Key {
            pressed: true,
            modifiers: event_modifiers,
            ..
        } = event
        {
            modifiers = *event_modifiers;
            break;
        }
    }
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
    if let Ok(mut latest) = app.shell.indexing.latest_request_ids.lock() {
        latest.clear();
    }
}

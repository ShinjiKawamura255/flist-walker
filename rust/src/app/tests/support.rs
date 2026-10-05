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
    strict_cleanup: bool,
    cleaned: bool,
    auxiliary_writer_path: Option<PathBuf>,
}

impl TestSettingsScope {
    pub(crate) fn new(name: &str) -> Self {
        let base = test_root(name);
        fs::create_dir_all(&base).expect("create test settings dir");
        Self {
            base,
            strict_cleanup: false,
            cleaned: false,
            auxiliary_writer_path: None,
        }
    }

    pub(crate) fn with_strict_cleanup(mut self) -> Self {
        self.strict_cleanup = true;
        self
    }

    fn track_auxiliary_writer(&mut self, path: PathBuf) {
        assert_eq!(path.parent(), Some(self.base.as_path()));
        assert!(!self.cleaned, "cannot add a writer after cleanup");
        assert!(self.auxiliary_writer_path.is_none(), "writer already owned");
        self.auxiliary_writer_path = Some(path);
    }

    fn cleanup(&mut self, timeout: std::time::Duration) -> Result<(), String> {
        if self.cleaned {
            return Ok(());
        }
        let deadline = std::time::Instant::now() + timeout;
        let mut result = crate::persistence::finish_ui_state_persistence_for_test(
            &FlistWalkerApp::ui_state_file_path_in(&self.base),
            deadline.saturating_duration_since(std::time::Instant::now()),
        );
        if let Some(path) = &self.auxiliary_writer_path {
            let auxiliary = crate::persistence::finish_ui_state_persistence_for_test(
                path,
                deadline.saturating_duration_since(std::time::Instant::now()),
            );
            if result.is_ok() {
                result = auxiliary;
            }
        }
        result?;
        fs::remove_dir_all(&self.base)
            .map_err(|error| format!("remove settings fixture {}: {error}", self.base.display()))?;
        self.cleaned = true;
        Ok(())
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
        if self.strict_cleanup {
            if let Err(error) = self.cleanup(std::time::Duration::from_secs(5)) {
                // Keep the fixture as failure evidence. Do not hide an earlier panic.
                if std::thread::panicking() {
                    eprintln!("settings fixture teardown failed: {error}");
                } else {
                    panic!("settings fixture teardown failed: {error}");
                }
            }
        } else {
            let _ = fs::remove_dir_all(&self.base);
        }
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

#[test]
fn perf_settings_cleanup_waits_for_the_app_writer_before_removing_its_root() {
    use std::time::Duration;
    let mut settings = TestSettingsScope::new("strict-settings-teardown").with_strict_cleanup();
    let path = FlistWalkerApp::ui_state_file_path_in(&settings.base);
    let root = settings.base.join("startup");
    fs::create_dir_all(&root).unwrap();
    let mut app = settings.app(root, 1000, String::new());
    let gate = crate::persistence::WriteGate::new(path.clone());
    crate::persistence::enqueue_ui_state_patch(
        path.clone(),
        crate::persistence::UiStatePatch::default(),
        vec![],
        false,
    )
    .unwrap();
    gate.wait_entered();
    app.shell.ui.show_preview = false;
    drop(app);
    assert!(settings.cleanup(Duration::from_millis(1)).is_err());
    assert!(
        settings.base.is_dir(),
        "unconfirmed writer termination must retain evidence"
    );
    // The retained writer cannot block another path's admission or termination.
    let other = settings.base.join("unrelated.json");
    settings.track_auxiliary_writer(other.clone());
    let other_gate = crate::persistence::WriteGate::new(other.clone());
    crate::persistence::enqueue_ui_state_patch(
        other.clone(),
        crate::persistence::UiStatePatch::default(),
        vec![],
        false,
    )
    .unwrap();
    other_gate.wait_entered();
    other_gate.release();
    crate::persistence::finish_ui_state_persistence_for_test(&other, Duration::from_secs(1))
        .expect("unrelated writer must physically stop after entering its write");
    assert_eq!(
        settings.cleanup(Duration::from_millis(1)).unwrap_err(),
        "test settings writer did not physically stop before deadline"
    );
    assert!(settings.base.is_dir(), "the first writer is still parked");
    gate.release();
    settings.cleanup(Duration::from_secs(2)).unwrap();
    assert!(!settings.base.exists());
    crate::persistence::finish_ui_state_persistence_for_test(&path, Duration::from_millis(1))
        .unwrap();
    assert!(
        !settings.base.exists(),
        "a physically joined writer cannot recreate the root"
    );
}

#[test]
fn perf_settings_cleanup_retains_the_root_until_its_auxiliary_writer_returns() {
    use std::time::Duration;
    let mut settings = TestSettingsScope::new("strict-auxiliary-teardown").with_strict_cleanup();
    let other = settings.base.join("unrelated.json");
    settings.track_auxiliary_writer(other.clone());
    let gate = crate::persistence::WriteGate::new(other.clone());
    crate::persistence::enqueue_ui_state_patch(
        other.clone(),
        crate::persistence::UiStatePatch::default(),
        vec![],
        false,
    )
    .unwrap();
    gate.wait_entered();
    let result = settings.cleanup(Duration::from_millis(1));
    let retained = settings.base.is_dir();
    gate.release();
    if result.is_ok() {
        // Dispose safely even when the old cleanup incorrectly claimed success.
        crate::persistence::finish_ui_state_persistence_for_test(&other, Duration::from_secs(2))
            .unwrap();
        if settings.base.exists() {
            fs::remove_dir_all(&settings.base).unwrap();
        }
        settings.cleaned = true;
    } else {
        settings.cleanup(Duration::from_secs(2)).unwrap();
    }
    assert_eq!(
        result.unwrap_err(),
        "test settings writer did not physically stop before deadline"
    );
    assert!(
        retained,
        "unconfirmed auxiliary writer must retain evidence"
    );
    assert!(!settings.base.exists());
}

#[test]
fn perf_settings_cleanup_reports_removal_failure_and_retains_the_fixture() {
    use std::time::Duration;
    let mut settings = TestSettingsScope::new("strict-settings-remove").with_strict_cleanup();
    fs::remove_dir(&settings.base).unwrap();
    fs::write(&settings.base, "failure evidence").unwrap();
    assert!(settings.cleanup(Duration::from_millis(1)).is_err());
    assert_eq!(
        fs::read_to_string(&settings.base).unwrap(),
        "failure evidence"
    );
    // Explicit test-owned disposal after checking the error, not a cleanup retry.
    fs::remove_file(&settings.base).unwrap();
    settings.cleaned = true;
}

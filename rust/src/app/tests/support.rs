use super::{egui, FlistWalkerApp, PathBuf};
use std::fs;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

pub(crate) fn test_root(name: &str) -> PathBuf {
    static NEXT_ROOT: AtomicU64 = AtomicU64::new(0);
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("clock")
        .as_nanos();
    // Wall-clock resolution alone does not distinguish concurrent fixtures.
    let sequence = NEXT_ROOT.fetch_add(1, Ordering::Relaxed);
    std::env::temp_dir().join(format!(
        "fff-rs-app-{name}-{nonce}-{:010}-{sequence:016x}",
        std::process::id()
    ))
}

#[test]
fn test_roots_are_unique_for_concurrent_same_name_fixtures() {
    let barrier = std::sync::Arc::new(std::sync::Barrier::new(8));
    let threads = (0..8)
        .map(|_| {
            let barrier = barrier.clone();
            std::thread::spawn(move || {
                barrier.wait();
                (0..128)
                    .map(|_| test_root("concurrent"))
                    .collect::<Vec<_>>()
            })
        })
        .collect::<Vec<_>>();
    let roots = threads
        .into_iter()
        .flat_map(|t| t.join().unwrap())
        .collect::<std::collections::HashSet<_>>();
    assert_eq!(roots.len(), 8 * 128);
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
    // Readiness includes the real write and result publication. The unchanged
    // one-second stop bound then tests physical termination, independently of I/O.
    let other_gate = crate::persistence::WriteGate::after_write(other.clone());
    let other_generation = crate::persistence::enqueue_ui_state_patch(
        other.clone(),
        crate::persistence::UiStatePatch::default(),
        vec![],
        false,
    )
    .unwrap();
    other_gate.wait_entered();
    let other_status = crate::persistence::ui_state_persistence_status(&other);
    assert_eq!(
        other_status.persisted_generation, other_generation,
        "unrelated writer readiness must observe the successful real write"
    );
    assert!(other_status.last_error.is_none());
    assert!(other.is_file(), "readiness follows real file publication");
    assert!(
        crate::persistence::finish_ui_state_persistence_for_test(&other, Duration::from_millis(1))
            .is_err(),
        "published status cannot substitute for a physically stopped writer"
    );
    assert!(settings.base.is_dir(), "held writers retain the same root");
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

// Drive only folder-picker preparation; completion is a state condition, not a delay.
pub(crate) fn settle_root_browse(app: &mut FlistWalkerApp) {
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    while app
        .shell
        .features
        .root_browser
        .browse
        .service
        .as_ref()
        .is_some_and(|s| s.in_progress())
    {
        app.poll_root_browse();
        assert!(
            std::time::Instant::now() < deadline,
            "Browse preparation did not settle"
        );
        std::thread::yield_now();
    }
}

// Diagnostic-only causal controls, never a reproduction or acceptance of a
// historical Windows deadline failure. No indexing input lives under settings.
#[test]
fn causal_control_writer_separate_fixture_cleanup() {
    use crate::persistence::{
        enqueue_ui_state_patch, finish_ui_state_persistence_for_test, flush_ui_state_persistence,
        ui_state_persistence_status, UiStatePatch, WriteGate,
    };
    use std::time::Duration;
    let mut a = TestSettingsScope::new("causal-same-name").with_strict_cleanup();
    let mut b = TestSettingsScope::new("causal-same-name").with_strict_cleanup();
    let a_path = FlistWalkerApp::ui_state_file_path_in(&a.base);
    let b_path = FlistWalkerApp::ui_state_file_path_in(&b.base);
    // a was declared before its gate, so unwind releases the gate before the
    // strict owner destructor attempts a bounded physical join.
    let gate = WriteGate::new(a_path.clone());
    let a_generation =
        enqueue_ui_state_patch(a_path.clone(), UiStatePatch::default(), vec![], false).unwrap();
    gate.wait_entered();
    let a_before = ui_state_persistence_status(&a_path);
    let b_generation =
        enqueue_ui_state_patch(b_path.clone(), UiStatePatch::default(), vec![], false).unwrap();
    let b_flush = flush_ui_state_persistence(&b_path, Duration::from_secs(2));
    let b_status = ui_state_persistence_status(&b_path);
    let b_file = b_path.is_file();
    let b_cleanup = b.cleanup(Duration::from_secs(2));
    let a_after = ui_state_persistence_status(&a_path);
    let a_still_owned = finish_ui_state_persistence_for_test(&a_path, Duration::from_millis(1));
    let a_root_retained = a.base.is_dir();
    gate.release();
    // An absent owner is never physical-return proof: keep A's evidence in
    // that unexpected branch instead of asking a no-owner cleanup to delete it.
    let a_cleanup = if a_still_owned.is_err() {
        a.cleanup(Duration::from_secs(2))
    } else {
        a.cleaned = true;
        Err("held A writer lost its registered ownership; root retained".into())
    };
    assert_ne!(a_path, b_path);
    b_flush.unwrap();
    assert_eq!(b_status.accepted_generation, b_generation);
    assert_eq!(b_status.persisted_generation, b_generation);
    assert!(b_file);
    b_cleanup.unwrap();
    assert_eq!(a_before.accepted_generation, a_generation);
    assert_eq!(a_before.persisted_generation, 0);
    assert_eq!(a_after, a_before);
    assert_eq!(
        a_still_owned.unwrap_err(),
        "test settings writer did not physically stop before deadline"
    );
    assert!(a_root_retained);
    a_cleanup.unwrap();
    assert!(!a.base.exists() && !b.base.exists());
    eprintln!("CAUSAL_WRITER_SEPARATE_SCOPES accepted_A={a_generation} saved_B={b_generation} A_retained_then_physically_joined=true");
}

#[test]
fn causal_control_writer_fixture_lifetime_counterfactual() {
    use crate::persistence::finish_ui_state_persistence_for_test;
    use std::time::Duration;
    let input = test_root("causal-lifetime-input");
    fs::create_dir_all(&input).unwrap();
    let mut correct = TestSettingsScope::new("causal-lifetime-correct").with_strict_cleanup();
    let correct_path = FlistWalkerApp::ui_state_file_path_in(&correct.base);
    let app = correct.app(input.clone(), 1000, String::new());
    drop(app);
    let correct_join = finish_ui_state_persistence_for_test(&correct_path, Duration::from_secs(2));
    let correct_file = correct_join.is_ok() && correct_path.is_file();
    let correct_saved: Option<serde_json::Value> = correct_join
        .as_ref()
        .ok()
        .and_then(|_| fs::read(&correct_path).ok())
        .and_then(|bytes| serde_json::from_slice(&bytes).ok());
    let saved_root = correct_saved
        .as_ref()
        .and_then(|value| value["last_root"].as_str())
        .and_then(|path| fs::canonicalize(path).ok());
    let expected_root = fs::canonicalize(&input).ok();
    let correct_cleanup = if correct_join.is_ok() {
        correct.cleanup(Duration::from_secs(2))
    } else {
        correct.cleaned = true; // preserve first failure and unconfirmed root
        Err("correct lifecycle writer not physically confirmed".into())
    };
    let correct_removed = !correct.base.exists();
    correct_join.unwrap();
    correct_cleanup.unwrap();
    assert!(
        correct_removed && correct_file && expected_root.is_some() && saved_root == expected_root,
        "normal lifecycle requires a real autosave with the expected root, then physical join and cleanup"
    );

    let mut premature = TestSettingsScope::new("causal-lifetime-premature").with_strict_cleanup();
    let path = FlistWalkerApp::ui_state_file_path_in(&premature.base);
    // Standby ownership precedes App construction; unwind drops App first.
    // It becomes active only after the deliberately premature owner finished.
    let mut final_owner = TestSettingsScope {
        base: premature.base.clone(),
        strict_cleanup: true,
        cleaned: true,
        auxiliary_writer_path: None,
    };
    let app = premature.app(input.clone(), 1000, String::new());
    let premature_cleanup = premature.cleanup(Duration::from_secs(2));
    let removed_before_app = !premature.base.exists();
    if premature_cleanup.is_ok() {
        final_owner.cleaned = false;
    }
    drop(app); // forced autosave after the deliberate unsupported cleanup
    let last_join = finish_ui_state_persistence_for_test(&path, Duration::from_secs(2));
    let recreated_after_app = premature.base.is_dir() && path.is_file();
    let final_cleanup = if last_join.is_ok() {
        if premature_cleanup.is_ok() {
            final_owner.cleanup(Duration::from_secs(2))
        } else {
            premature.cleanup(Duration::from_secs(2))
        }
    } else {
        final_owner.cleaned = true;
        premature.cleaned = true;
        Err("counterfactual writer not physically confirmed; root retained".into())
    };
    // This input root was never an early-cleanup target and stayed live through
    // both App drops. No setting-writer evidence is deleted on an unconfirmed join.
    let input_retained = input.is_dir();
    if last_join.is_ok() {
        fs::remove_dir_all(&input).unwrap();
    }
    premature_cleanup.unwrap();
    last_join.unwrap();
    final_cleanup.unwrap();
    assert!(correct_removed && removed_before_app && recreated_after_app && input_retained);
    assert!(!premature.base.exists());
    eprintln!("CAUSAL_WRITER_LIFETIME correct_order_removed=true premature_scope_then_App_recreated=true final_physical_join=true; unsupported_counterfactual_not_natural_reproduction");
}

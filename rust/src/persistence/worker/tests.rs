use super::*;
use crate::fs_atomic::acquire_sidecar_lock;
use crate::persistence::paths::{
    migrate_or_legacy_path, migrate_or_legacy_saved_roots_path, saved_roots_file_path_in,
    ui_state_file_path_in,
};
use serde_json::json;
use std::env;
use std::fs;
use std::process::Command;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

#[test]
fn tc_168_typed_invalid_document_is_preserved_before_settings_side_effects() {
    for invalid in [
        json!({"show_preview": "invalid", "default_root": "valuable"}),
        json!({"window": {"width": 100}}),
        json!({"tabs": [{"root": "valuable"}]}),
    ] {
        let base = temp_dir("typed-invalid-write");
        fs::create_dir_all(&base).unwrap();
        let path = base.join("state.json");
        let roots = base.join("roots.txt");
        let original = invalid.to_string();
        fs::write(&path, &original).unwrap();
        fs::write(&roots, "valuable roots").unwrap();
        let result = commit_settings(
            &path,
            &[],
            false,
            Duration::from_millis(10),
            SettingsCommitRequest {
                request_id: 1,
                patch: UiStatePatch::from_ui_state(&UiState::default()),
                saved_roots: Some((roots.clone(), "replacement".into())),
            },
        );
        assert!(
            result.is_err(),
            "typed-invalid data must not grant write permission"
        );
        assert_eq!(fs::read_to_string(&path).unwrap(), original);
        assert_eq!(fs::read_to_string(&roots).unwrap(), "valuable roots");
        fs::remove_dir_all(base).unwrap();
    }
}

#[test]
fn tc_168_sustained_failure_has_bounded_nonblocking_admission() {
    let base = temp_dir("bounded-admission");
    fs::create_dir_all(&base).unwrap();
    let path = base.join("state.json");
    fs::write(&path, "{").unwrap();
    let writer = AsyncHistoryPersistence::new_with_lock_timeout(
        path.clone(),
        false,
        Duration::from_millis(10),
    );
    let mut admitted = 0;
    for index in 0..10_000 {
        if writer
            .enqueue_history(vec![format!("query-{index}")])
            .is_ok()
        {
            admitted += 1;
        }
    }
    assert!(admitted <= 64, "failure backlog admitted {admitted} writes");
    assert!(writer.shutdown(Duration::from_secs(1)).is_err());
    fs::remove_dir_all(base).unwrap();
}

#[test]
fn tc_168_failed_startup_read_never_autosaves_fallback_after_external_repair() {
    let base = temp_dir("startup-fallback-provenance");
    fs::create_dir_all(&base).unwrap();
    let path = base.join("state.json");
    fs::write(
        &path,
        json!({"show_preview": "invalid", "default_root": "valuable"}).to_string(),
    )
    .unwrap();
    let fallback = crate::persistence::read_ui_state_from_path(&path);
    assert!(fallback.default_root.is_none());
    let repaired =
        json!({"show_preview": true, "default_root": "valuable", "future": 42}).to_string();
    fs::write(&path, &repaired).unwrap();
    let _ = enqueue_ui_state_patch(
        path.clone(),
        UiStatePatch::from_ui_state(&fallback),
        Vec::new(),
        false,
    );
    let _ = flush_ui_state_persistence(&path, Duration::from_secs(1));
    assert_eq!(fs::read_to_string(&path).unwrap(), repaired);
    shutdown_ui_state_persistence_for_test(&path, Duration::from_secs(1));
    fs::remove_dir_all(base).unwrap();
}

#[test]
fn tc_168_compacted_history_delta_preserves_sequential_recency_and_cap() {
    let initial = (0..100)
        .map(|index| format!("q-{index}"))
        .collect::<Vec<_>>();
    let deltas = (0..1_000)
        .map(|index| format!("q-{}", (index * 37) % 151))
        .collect::<Vec<_>>();
    let mut sequential = initial.clone();
    for delta in &deltas {
        append_history_delta(&mut sequential, delta.clone());
    }
    let compacted = normalize_history_recency(deltas);
    assert_eq!(compacted.len(), MAX_QUERY_HISTORY_ENTRIES);
    let mut replayed = initial;
    for delta in compacted {
        append_history_delta(&mut replayed, delta);
    }
    assert_eq!(replayed, sequential);
}

#[test]
fn tc_168_rejected_snapshot_keeps_history_baseline_and_recovers_latest_generation() {
    let base = temp_dir("full-snapshot-retry");
    fs::create_dir_all(&base).unwrap();
    let path = base.join("state.json");
    // No failed startup read: these are legitimate changes made before a later
    // external corruption, so repairing the file may resume this session.
    fs::write(&path, "{}").unwrap();
    crate::persistence::read_ui_state_from_path(&path);
    fs::write(&path, "{").unwrap();
    for index in 0..MAX_PENDING_UI_STATE_WRITES {
        enqueue_ui_state_patch(
            path.clone(),
            UiStatePatch::from_json(json!({"latest": index})),
            vec!["accepted".into()],
            false,
        )
        .unwrap();
    }
    let rejection = enqueue_ui_state_patch(
        path.clone(),
        UiStatePatch::from_json(json!({"latest": 999})),
        vec!["accepted".into(), "retry".into()],
        false,
    )
    .unwrap_err();
    assert!(rejection.contains("full"));
    assert!(flush_ui_state_persistence(&path, Duration::from_secs(2)).is_err());
    let failed = ui_state_persistence_status(&path);
    assert_eq!(failed.accepted_generation, 64);
    assert_eq!(failed.persisted_generation, 0);
    assert!(failed.last_error.is_some());
    assert!(!failed.startup_protected);
    fs::write(
        &path,
        json!({"query_history": ["external"], "future": {"keep": true}}).to_string(),
    )
    .unwrap();
    flush_ui_state_persistence(&path, Duration::from_secs(2)).unwrap();
    let generation = enqueue_ui_state_patch(
        path.clone(),
        UiStatePatch::from_json(json!({"latest": 999})),
        vec!["accepted".into(), "retry".into()],
        false,
    )
    .unwrap();
    flush_ui_state_persistence(&path, Duration::from_secs(2)).unwrap();
    let status = ui_state_persistence_status(&path);
    assert_eq!(status.accepted_generation, generation);
    assert_eq!(status.persisted_generation, generation);
    assert!(status.last_error.is_none());
    let document: Value = serde_json::from_str(&fs::read_to_string(&path).unwrap()).unwrap();
    assert_eq!(
        document["query_history"],
        json!(["external", "accepted", "retry"])
    );
    assert_eq!(document["latest"], 999);
    assert_eq!(document["future"], json!({"keep": true}));
    shutdown_ui_state_persistence_for_test(&path, Duration::from_secs(1));
    fs::remove_dir_all(base).unwrap();
}

#[test]
fn tc_168_failed_startup_read_blocks_settings_after_repair_and_keeps_roots() {
    let base = temp_dir("startup-settings-protection");
    fs::create_dir_all(&base).unwrap();
    let path = base.join("state.json");
    let roots = base.join("roots.txt");
    fs::write(&path, "{\"show_preview\":42}").unwrap();
    fs::write(&roots, "valuable").unwrap();
    crate::persistence::read_ui_state_from_path(&path);
    fs::write(&path, "{}").unwrap();
    assert!(ui_state_persistence_status(&path).startup_protected);
    assert!(enqueue_settings_commit(
        path.clone(),
        false,
        SettingsCommitRequest {
            request_id: 1,
            patch: UiStatePatch::from_ui_state(&UiState::default()),
            saved_roots: Some((roots.clone(), "replacement".into())),
        }
    )
    .is_err());
    assert_eq!(fs::read_to_string(&roots).unwrap(), "valuable");
    assert_eq!(fs::read_to_string(&path).unwrap(), "{}");
    shutdown_ui_state_persistence_for_test(&path, Duration::from_secs(1));
    fs::remove_dir_all(base).unwrap();
}

#[test]
fn tc_168_disconnected_admission_never_claims_a_generation() {
    let (tx, rx) = mpsc::sync_channel(1);
    drop(rx);
    let sender = PersistenceSender {
        tx,
        state: Arc::new(Mutex::new(AdmissionState::default())),
    };
    assert!(sender
        .enqueue(UiStatePatch::default(), vec!["retry".into()])
        .unwrap_err()
        .contains("unavailable"));
    let state = sender.state.lock().unwrap();
    assert_eq!(state.outstanding, 0);
    assert_eq!(state.status.accepted_generation, 0);
    assert!(state.status.last_error.is_some());
}

#[test]
fn tc_168_old_success_cannot_clear_failure_while_new_generation_is_pending() {
    let state = Mutex::new(AdmissionState {
        status: UiStatePersistenceStatus {
            accepted_generation: 2,
            last_error: Some("failure".into()),
            ..Default::default()
        },
        outstanding: 2,
    });
    let mut pending = vec![PendingUiStateWrite {
        generation: 1,
        patch: UiStatePatch::default(),
        history_delta: Vec::new(),
    }];
    publish_write_result(&state, &mut pending, &Ok(()), true);
    assert_eq!(state.lock().unwrap().status.persisted_generation, 1);
    assert_eq!(
        state.lock().unwrap().status.last_error.as_deref(),
        Some("failure")
    );
    pending.push(PendingUiStateWrite {
        generation: 2,
        patch: UiStatePatch::default(),
        history_delta: Vec::new(),
    });
    publish_write_result(&state, &mut pending, &Ok(()), true);
    assert_eq!(state.lock().unwrap().status.persisted_generation, 2);
    assert!(state.lock().unwrap().status.last_error.is_none());
}

#[test]
fn tc_168_settings_flush_and_shutdown_preserve_admission_barriers() {
    let base = temp_dir("ordered-barriers");
    fs::create_dir_all(&base).unwrap();
    let path = base.join("state.json");
    let (tx, rx) = mpsc::sync_channel(UI_STATE_COMMAND_CAPACITY);
    let state = Arc::new(Mutex::new(AdmissionState::default()));
    let sender = PersistenceSender {
        tx,
        state: Arc::clone(&state),
    };
    sender
        .enqueue(
            UiStatePatch::from_json(json!({"show_preview": false})),
            vec!["A".into()],
        )
        .unwrap();
    let (settings_tx, settings_rx) = mpsc::channel();
    sender
        .send_control(UiStatePersistenceCommand::CommitSettings {
            request: SettingsCommitRequest {
                request_id: 7,
                patch: UiStatePatch::from_json(json!({"show_preview": true})),
                saved_roots: None,
            },
            response: settings_tx,
        })
        .unwrap();
    sender
        .enqueue(
            UiStatePatch::from_json(json!({"future": "second"})),
            vec!["B".into()],
        )
        .unwrap();
    let (flush_tx, flush_rx) = mpsc::channel();
    sender
        .send_control(UiStatePersistenceCommand::Flush(flush_tx))
        .unwrap();
    sender
        .enqueue(
            UiStatePatch::from_json(json!({"show_preview": "invalid"})),
            vec!["C".into()],
        )
        .unwrap();
    let (shutdown_tx, shutdown_rx) = mpsc::channel();
    sender
        .send_control(UiStatePersistenceCommand::Shutdown(shutdown_tx))
        .unwrap();
    // Queue all barriers before starting the worker so scheduling cannot make
    // a later invalid write appear to belong to an earlier flush/commit.
    let worker_path = path.clone();
    let worker_state = Arc::clone(&state);
    let handle = thread::spawn(move || {
        run_ui_state_persistence_worker(
            rx,
            worker_path,
            false,
            Duration::from_millis(10),
            worker_state,
        )
    });
    let settings = settings_rx.recv_timeout(Duration::from_secs(2)).unwrap();
    assert_eq!(settings.request_id, 7);
    settings.result.unwrap();
    flush_rx
        .recv_timeout(Duration::from_secs(2))
        .unwrap()
        .unwrap();
    assert!(shutdown_rx
        .recv_timeout(Duration::from_secs(2))
        .unwrap()
        .is_err());
    handle.join().unwrap();
    let document: Value = serde_json::from_str(&fs::read_to_string(&path).unwrap()).unwrap();
    assert_eq!(document["show_preview"], true);
    assert_eq!(document["future"], "second");
    assert_eq!(document["query_history"], json!(["A", "B"]));
    let status = &state.lock().unwrap().status;
    assert_eq!(status.persisted_generation, 2);
    assert_eq!(status.accepted_generation, 3);
    assert!(status.last_error.is_some());
    fs::remove_dir_all(base).unwrap();
}

#[test]
fn tc_168_control_channel_is_bounded_and_full_is_explicit() {
    let (tx, _rx) = mpsc::sync_channel(UI_STATE_COMMAND_CAPACITY);
    let sender = PersistenceSender {
        tx,
        state: Arc::new(Mutex::new(AdmissionState::default())),
    };
    for _ in 0..UI_STATE_COMMAND_CAPACITY {
        let (reply, _) = mpsc::channel();
        sender
            .send_control(UiStatePersistenceCommand::Flush(reply))
            .unwrap();
    }
    let (reply, _) = mpsc::channel();
    assert!(sender
        .send_control(UiStatePersistenceCommand::Shutdown(reply))
        .unwrap_err()
        .contains("full"));
}

#[test]
fn public_persistence_round_trip_preserves_existing_document_fields() {
    let base = temp_dir("public-round-trip");
    fs::create_dir_all(&base).expect("create base");
    let state_path = base.join("ui-state.json");
    let roots_path = base.join("roots.txt");
    let last_root = base.join("last-root");
    fs::write(
        &state_path,
        json!({
            "last_root": last_root,
            "query_history": ["old"],
            "unknown_future_field": {"keep": true},
            "show_preview": false
        })
        .to_string(),
    )
    .expect("seed state");
    fs::write(&roots_path, "saved-root\n").expect("seed roots");

    let writer = crate::persistence::AsyncHistoryPersistence::new(state_path.clone(), false);
    writer.enqueue_history(vec!["new".into()]).expect("enqueue");
    writer.flush(Duration::from_secs(2)).expect("flush");
    writer.shutdown(Duration::from_secs(2)).expect("shutdown");
    let loaded = crate::persistence::load_persisted_roots_and_history_from_paths(
        &state_path,
        &roots_path,
        false,
    );
    assert_eq!(
        loaded.last_root,
        Some(normalize_windows_path_buf(last_root))
    );
    assert_eq!(loaded.saved_roots, vec![PathBuf::from("saved-root")]);
    assert_eq!(loaded.query_history, vec!["old", "new"]);
    let document: Value =
        serde_json::from_str(&fs::read_to_string(&state_path).expect("read document"))
            .expect("parse document");
    assert_eq!(document["unknown_future_field"], json!({"keep": true}));
    assert_eq!(document["show_preview"], false);
    fs::remove_dir_all(base).expect("remove fixture");
}

#[test]
fn default_persistence_paths_are_isolated_in_unit_tests() {
    assert!(ui_state_file_path().is_none());
    assert!(crate::persistence::saved_roots_file_path().is_none());
    assert!(crate::persistence::AsyncHistoryPersistence::new_default().is_none());
    assert_eq!(
        crate::persistence::load_persisted_roots_and_history(),
        crate::persistence::PersistedRootsAndHistory::default()
    );
}

#[test]
fn public_persisted_reader_preserves_full_document_validation() {
    let base = temp_dir("wire-schema-compatibility");
    fs::create_dir_all(&base).expect("create base");
    let state_path = base.join("ui-state.json");
    let roots_path = base.join("roots.txt");
    fs::write(&roots_path, "saved-root\n").expect("write roots");
    let valid = json!({
        "last_root": "last-root",
        "default_root": "default-root",
        "query_history": ["first", "second"],
        "unknown_future_field": {"keep": true}
    });
    fs::write(&state_path, valid.to_string()).expect("write valid state");
    let loaded = crate::persistence::load_persisted_roots_and_history_from_paths(
        &state_path,
        &roots_path,
        false,
    );
    assert_eq!(loaded.last_root, Some(PathBuf::from("last-root")));
    assert_eq!(loaded.default_root, Some(PathBuf::from("default-root")));
    assert_eq!(loaded.query_history, vec!["first", "second"]);
    assert_eq!(loaded.saved_roots, vec![PathBuf::from("saved-root")]);

    let mut malformed_window = valid.clone();
    malformed_window["window"] = json!({"width": 100.0});
    let mut malformed_tabs = valid;
    malformed_tabs["tabs"] = json!([{
        "root": "root", "use_filelist": false, "use_regex": false,
        "include_files": true, "include_dirs": true, "query": "",
        "tab_accent": "unknown"
    }]);
    for document in [
        malformed_window.to_string(),
        malformed_tabs.to_string(),
        "{".into(),
    ] {
        fs::write(&state_path, document).expect("write malformed state");
        let loaded = crate::persistence::load_persisted_roots_and_history_from_paths(
            &state_path,
            &roots_path,
            false,
        );
        assert_eq!(loaded.last_root, None);
        assert_eq!(loaded.default_root, None);
        assert!(loaded.query_history.is_empty());
        assert_eq!(loaded.saved_roots, vec![PathBuf::from("saved-root")]);
    }
    fs::remove_dir_all(&base).expect("remove fixture");
}

#[test]
fn ui_state_file_path_in_joins_base_directory() {
    let base = PathBuf::from("/tmp/flistwalker-settings");
    assert_eq!(
        ui_state_file_path_in(&base),
        base.join(".flistwalker_ui_state.json")
    );
}

#[test]
fn saved_roots_file_path_in_joins_base_directory() {
    let base = PathBuf::from("/tmp/flistwalker-settings");
    assert_eq!(
        saved_roots_file_path_in(&base),
        base.join(".flistwalker_roots.txt")
    );
}

fn temp_dir(name: &str) -> PathBuf {
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("clock")
        .as_nanos();
    env::temp_dir().join(format!("flistwalker-session-{name}-{nonce}"))
}

#[test]
fn migrate_or_legacy_ui_state_path_prefers_current_and_moves_legacy_when_missing() {
    let base = temp_dir("ui-state");
    let legacy_base = base.join("legacy");
    let current_base = base.join("current");
    fs::create_dir_all(&legacy_base).expect("create legacy");
    fs::create_dir_all(&current_base).expect("create current");
    let current_path = ui_state_file_path_in(&current_base);
    let legacy_path = ui_state_file_path_in(&legacy_base);
    fs::write(&legacy_path, "{\"ignore_list_enabled\":false}").expect("write legacy");

    let resolved = migrate_or_legacy_path(&current_path, std::slice::from_ref(&legacy_path));
    assert_eq!(resolved, current_path);
    assert!(current_path.exists());
    assert!(!legacy_path.exists());

    let _ = fs::remove_dir_all(&base);
}

#[test]
fn migrate_or_legacy_saved_roots_path_leaves_existing_current_file_untouched() {
    let base = temp_dir("saved-roots");
    let legacy_base = base.join("legacy");
    let current_base = base.join("current");
    fs::create_dir_all(&legacy_base).expect("create legacy");
    fs::create_dir_all(&current_base).expect("create current");
    let current_path = saved_roots_file_path_in(&current_base);
    let legacy_path = saved_roots_file_path_in(&legacy_base);
    fs::write(&legacy_path, "legacy-root").expect("write legacy");
    fs::write(&current_path, "current-root").expect("write current");

    let resolved = migrate_or_legacy_saved_roots_path(&current_path);
    assert_eq!(resolved, current_path);
    assert!(current_path.exists());
    assert!(legacy_path.exists());

    let _ = fs::remove_dir_all(&base);
}

#[test]
fn migrate_or_legacy_path_skips_missing_legacy_and_uses_next_one() {
    let base = temp_dir("migration-priority");
    let current_base = base.join("current");
    let missing_legacy_base = base.join("missing-legacy");
    let legacy_base = base.join("legacy");
    fs::create_dir_all(&current_base).expect("create current");
    fs::create_dir_all(&legacy_base).expect("create legacy");
    let current_path = ui_state_file_path_in(&current_base);
    let missing_legacy_path = ui_state_file_path_in(&missing_legacy_base);
    let legacy_path = ui_state_file_path_in(&legacy_base);
    fs::write(&legacy_path, "{\"ignore_list_enabled\":false}").expect("write legacy");

    let resolved =
        migrate_or_legacy_path(&current_path, &[missing_legacy_path, legacy_path.clone()]);
    assert_eq!(resolved, current_path);
    assert!(current_path.exists());
    assert!(!legacy_path.exists());

    let _ = fs::remove_dir_all(&base);
}

#[test]
fn tc_167_persistence_merges_two_writers_and_preserves_unknown_json_fields() {
    let base = temp_dir("two-writers");
    let path = base.join("ui-state.json");
    fs::create_dir_all(&base).expect("create base");
    fs::write(
        &path,
        json!({
            "unknown_top": {"keep": true},
            "window": {"x": 1.0, "y": 2.0, "width": 700.0, "height": 500.0, "unknown_nested": "keep"},
            "query_history": []
        })
        .to_string(),
    )
    .expect("seed state");
    let writer_a = AsyncHistoryPersistence::new_with_lock_timeout(
        path.clone(),
        false,
        Duration::from_millis(50),
    );
    let writer_b = AsyncHistoryPersistence::new_with_lock_timeout(
        path.clone(),
        false,
        Duration::from_millis(50),
    );

    writer_a.enqueue_patch_for_test(
        UiStatePatch::from_json(json!({"window": {"width": 800.0}})),
        vec!["alpha".into()],
    );
    writer_a
        .flush(Duration::from_secs(1))
        .expect("flush writer a");
    writer_b.enqueue_patch_for_test(
        UiStatePatch::from_json(json!({"show_preview": false})),
        vec!["beta".into()],
    );
    writer_b
        .flush(Duration::from_secs(1))
        .expect("flush writer b");

    let written: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(&path).expect("read state")).expect("parse state");
    assert_eq!(written["unknown_top"]["keep"], true);
    assert_eq!(written["window"]["unknown_nested"], "keep");
    assert_eq!(written["window"]["width"], 800.0);
    assert_eq!(written["query_history"], json!(["alpha", "beta"]));
    writer_a
        .shutdown(Duration::from_secs(1))
        .expect("shutdown writer a");
    writer_b
        .shutdown(Duration::from_secs(1))
        .expect("shutdown writer b");
    let _ = fs::remove_dir_all(&base);
}

#[test]
fn tc_167_persistence_keeps_ordered_a_b_a_deltas_as_b_a() {
    let base = temp_dir("history-burst");
    let path = base.join("ui-state.json");
    let writer = AsyncHistoryPersistence::new_with_lock_timeout(
        path.clone(),
        false,
        Duration::from_millis(50),
    );

    writer.enqueue_patch_for_test(UiStatePatch::default(), vec!["A".into()]);
    writer.enqueue_patch_for_test(UiStatePatch::default(), vec!["B".into()]);
    writer.enqueue_patch_for_test(UiStatePatch::default(), vec!["A".into()]);
    writer
        .flush(Duration::from_secs(1))
        .expect("flush history burst");

    let written: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(&path).expect("read state")).expect("parse state");
    assert_eq!(written["query_history"], json!(["B", "A"]));
    writer
        .shutdown(Duration::from_secs(1))
        .expect("shutdown writer");
    let _ = fs::remove_dir_all(&base);
}

#[test]
fn tc_167_persistence_deduplicates_and_caps_history_at_100() {
    let base = temp_dir("history-cap");
    let path = base.join("ui-state.json");
    let writer = AsyncHistoryPersistence::new_with_lock_timeout(
        path.clone(),
        false,
        Duration::from_millis(50),
    );

    writer.enqueue_patch_for_test(
        UiStatePatch::default(),
        (0..101).map(|index| format!("q-{index}")).collect(),
    );
    writer.enqueue_patch_for_test(UiStatePatch::default(), vec!["q-50".into()]);
    writer
        .flush(Duration::from_secs(1))
        .expect("flush capped history");

    let written: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(&path).expect("read state")).expect("parse state");
    let history = written["query_history"].as_array().expect("history array");
    assert_eq!(history.len(), 100);
    assert_eq!(history.first(), Some(&json!("q-1")));
    assert_eq!(history.last(), Some(&json!("q-50")));
    writer
        .shutdown(Duration::from_secs(1))
        .expect("shutdown writer");
    let _ = fs::remove_dir_all(&base);
}

#[test]
fn tc_167_persistence_coalesces_patch_leaves_last_write_wins() {
    let base = temp_dir("patch-leaves");
    let path = base.join("ui-state.json");
    let writer = AsyncHistoryPersistence::new_with_lock_timeout(
        path.clone(),
        false,
        Duration::from_millis(50),
    );

    writer.enqueue_patch_for_test(
        UiStatePatch::from_json(json!({"show_preview": false, "window": {"x": 0.0, "y": 0.0, "width": 800.0, "height": 500.0}})),
        Vec::new(),
    );
    writer.enqueue_patch_for_test(
        UiStatePatch::from_json(json!({"show_preview": true, "window": {"height": 600.0}})),
        Vec::new(),
    );
    writer
        .flush(Duration::from_secs(1))
        .expect("flush patch leaves");

    let written: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(&path).expect("read state")).expect("parse state");
    assert_eq!(written["show_preview"], true);
    assert_eq!(written["window"]["width"], 800.0);
    assert_eq!(written["window"]["height"], 600.0);
    writer
        .shutdown(Duration::from_secs(1))
        .expect("shutdown writer");
    let _ = fs::remove_dir_all(&base);
}

#[test]
fn tc_167_persistence_retries_lock_timeout_without_losing_generations() {
    let base = temp_dir("retry");
    let path = base.join("ui-state.json");
    fs::create_dir_all(&base).expect("create base");
    let lock = acquire_sidecar_lock(&path, Duration::from_millis(10)).expect("hold lock");
    let writer = AsyncHistoryPersistence::new_with_lock_timeout(
        path.clone(),
        false,
        Duration::from_millis(10),
    );

    writer.enqueue_patch_for_test(
        UiStatePatch::from_json(json!({"first": 1})),
        vec!["A".into()],
    );
    assert!(writer.flush(Duration::from_millis(200)).is_err());
    writer.enqueue_patch_for_test(
        UiStatePatch::from_json(json!({"second": 2})),
        vec!["B".into()],
    );
    drop(lock);
    writer
        .flush(Duration::from_secs(1))
        .expect("retry after release");

    let written: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(&path).expect("read state")).expect("parse state");
    assert_eq!(written["first"], 1);
    assert_eq!(written["second"], 2);
    assert_eq!(written["query_history"], json!(["A", "B"]));
    writer
        .shutdown(Duration::from_secs(1))
        .expect("shutdown writer");
    let _ = fs::remove_dir_all(&base);
}

#[test]
fn tc_167_persistence_disabled_history_is_a_load_and_save_noop() {
    let base = temp_dir("history-disabled");
    let path = base.join("ui-state.json");
    fs::create_dir_all(&base).expect("create base");
    fs::write(
        &path,
        json!({"query_history": ["old"], "unknown": true}).to_string(),
    )
    .expect("seed state");
    let writer = AsyncHistoryPersistence::new_with_lock_timeout(
        path.clone(),
        true,
        Duration::from_millis(50),
    );

    writer.enqueue_patch_for_test(
        UiStatePatch::from_json(json!({"show_preview": false})),
        vec!["new".into()],
    );
    writer
        .flush(Duration::from_secs(1))
        .expect("flush disabled history");

    let written: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(&path).expect("read state")).expect("parse state");
    assert_eq!(written["query_history"], json!(["old"]));
    assert_eq!(written["unknown"], true);
    writer
        .shutdown(Duration::from_secs(1))
        .expect("shutdown writer");
    let _ = fs::remove_dir_all(&base);
}

#[test]
fn tc_168_persistence_enqueue_does_not_wait_for_a_held_lock() {
    let base = temp_dir("frame-latency");
    let path = base.join("ui-state.json");
    fs::create_dir_all(&base).expect("create base");
    let lock = acquire_sidecar_lock(&path, Duration::from_millis(10)).expect("hold lock");
    let writer =
        AsyncHistoryPersistence::new_with_lock_timeout(path.clone(), false, Duration::from_secs(1));

    let started = Instant::now();
    writer.enqueue_patch_for_test(
        UiStatePatch::from_json(json!({"frame": "fast"})),
        Vec::new(),
    );
    assert!(started.elapsed() < Duration::from_millis(200));
    drop(lock);
    writer
        .flush(Duration::from_secs(1))
        .expect("flush after lock release");

    writer
        .shutdown(Duration::from_secs(1))
        .expect("shutdown writer");
    let _ = fs::remove_dir_all(&base);
}

#[test]
fn tc_168_detached_ui_state_writer_flushes_outside_frame_waiting_for_lock_release() {
    let base = temp_dir("detached-flush");
    let path = base.join("ui-state.json");
    fs::create_dir_all(&base).expect("create base");
    let lock = acquire_sidecar_lock(&path, Duration::from_millis(10)).expect("hold lock");

    let started = Instant::now();
    enqueue_ui_state_patch(
        path.clone(),
        UiStatePatch::from_json(json!({"frame": "enqueued"})),
        Vec::new(),
        false,
    )
    .expect("admit patch");
    assert!(started.elapsed() < Duration::from_millis(200));
    drop(lock);
    flush_ui_state_persistence(&path, Duration::from_secs(1)).expect("flush patch");

    let written: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(&path).expect("read state")).expect("parse state");
    assert_eq!(written["frame"], "enqueued");
    let _ = fs::remove_dir_all(&base);
}

#[test]
fn tc_167_observed_settings_commit_reports_saved_root_failure_without_changing_ui_state() {
    let base = temp_dir("observed-settings-failure");
    let ui_state_path = base.join("ui-state.json");
    let invalid_roots_target = base.join("roots-as-directory");
    fs::create_dir_all(&invalid_roots_target).expect("create invalid target directory");
    fs::write(
        &ui_state_path,
        json!({"default_root": "old-root", "unknown": true}).to_string(),
    )
    .expect("seed state");
    let before = fs::read(&ui_state_path).expect("read state before commit");

    let response = enqueue_settings_commit(
        ui_state_path.clone(),
        false,
        SettingsCommitRequest {
            request_id: 41,
            patch: UiStatePatch::from_json(json!({"default_root": "new-root"})),
            saved_roots: Some((invalid_roots_target, "new-root\n".to_string())),
        },
    )
    .expect("enqueue observed commit")
    .recv_timeout(Duration::from_secs(1))
    .expect("observed response");

    assert_eq!(response.request_id, 41);
    assert!(response.result.is_err());
    assert_eq!(
        fs::read(&ui_state_path).expect("read state after failure"),
        before
    );
    let _ = fs::remove_dir_all(&base);
}

#[test]
fn tc_167_observed_settings_commit_rolls_back_saved_roots_when_ui_state_write_fails() {
    let base = temp_dir("observed-settings-rollback");
    let ui_state_path = base.join("ui-state.json");
    let roots_path = base.join("roots.txt");
    fs::create_dir_all(&base).expect("create base");
    fs::write(&ui_state_path, "{\"unknown\":true}").expect("seed UI state");
    fs::write(&roots_path, "old-root\n").expect("seed roots");
    let mut writes = Vec::new();

    let result = commit_settings_with_writer(
        &ui_state_path,
        &[],
        false,
        Duration::from_secs(1),
        SettingsCommitRequest {
            request_id: 42,
            patch: UiStatePatch::from_json(json!({"default_root": "new-root"})),
            saved_roots: Some((roots_path.clone(), "new-root\n".to_string())),
        },
        |path, bytes| {
            writes.push(path.to_path_buf());
            if path == ui_state_path {
                assert_eq!(fs::read_to_string(&roots_path).unwrap(), "new-root\n");
                Err(std::io::Error::other("injected UI-state write failure"))
            } else {
                write_bytes_atomic(path, bytes)
            }
        },
    );

    assert!(result
        .err()
        .unwrap()
        .to_string()
        .contains("injected UI-state write failure"));
    assert_eq!(
        writes,
        vec![
            roots_path.clone(),
            ui_state_path.clone(),
            roots_path.clone()
        ]
    );
    assert_eq!(
        fs::read_to_string(&ui_state_path).unwrap(),
        "{\"unknown\":true}"
    );
    assert_eq!(
        fs::read_to_string(&roots_path).expect("read rolled-back roots"),
        "old-root\n"
    );
    let _ = fs::remove_dir_all(&base);
}

#[test]
fn tc_167_unreadable_existing_document_is_not_an_empty_merge_base() {
    let base = temp_dir("unreadable-merge-base");
    let path = base.join("ui-state.json");
    fs::create_dir_all(&path).unwrap();
    let error = build_ui_state_document(&path, &[], None, false)
        .expect_err("existing unreadable targets must fail before preparing a replacement");
    assert_ne!(error.kind(), std::io::ErrorKind::NotFound);
    assert!(path.is_dir());
    fs::remove_dir_all(base).unwrap();
}

#[test]
fn tc_167_invalid_document_blocks_settings_commit_before_any_write() {
    for bytes in [b"{\"unknown\":".as_slice(), b"", b"null", b"[]", b"\xff"] {
        let base = temp_dir("invalid-settings-document");
        fs::create_dir_all(&base).unwrap();
        let path = base.join("ui-state.json");
        let roots = base.join("roots.txt");
        fs::write(&path, bytes).unwrap();
        fs::write(&roots, "old-root\n").unwrap();
        let mut writes = 0;
        let result = commit_settings_with_writer(
            &path,
            &[],
            false,
            Duration::from_secs(1),
            SettingsCommitRequest {
                request_id: 45,
                patch: UiStatePatch::from_json(json!({"show_preview": false})),
                saved_roots: Some((roots.clone(), "new-root\n".into())),
            },
            |target, bytes| {
                writes += 1;
                write_bytes_atomic(target, bytes)
            },
        );
        assert!(
            result.is_err(),
            "invalid existing bytes must be preserved: {bytes:?}"
        );
        assert_eq!(writes, 0);
        assert_eq!(fs::read(&path).unwrap(), bytes);
        assert_eq!(fs::read_to_string(&roots).unwrap(), "old-root\n");
        fs::remove_dir_all(base).unwrap();
    }
}

#[test]
fn tc_167_invalid_document_retry_preserves_history_until_external_repair() {
    let base = temp_dir("invalid-history-document");
    fs::create_dir_all(&base).unwrap();
    let path = base.join("ui-state.json");
    let original = b"{\"query_history\":[\"old\"],\"unknown\":";
    fs::write(&path, original).unwrap();
    let writer = AsyncHistoryPersistence::new(path.clone(), false);
    writer.enqueue_history(vec![" A ".into()]).unwrap();
    let error = writer
        .flush(Duration::from_secs(2))
        .expect_err("invalid JSON must fail");
    assert!(error.contains("UI-state"), "{error}");
    assert_eq!(fs::read(&path).unwrap(), original);
    writer
        .enqueue_history(vec!["B".into(), "A".into()])
        .unwrap();
    {
        let _lock = acquire_sidecar_lock(&path, Duration::from_secs(2)).unwrap();
        fs::write(
            &path,
            r#"{"query_history":["old"],"unknown":{"keep":true}}"#,
        )
        .unwrap();
    }
    writer.flush(Duration::from_secs(2)).unwrap();
    writer.shutdown(Duration::from_secs(2)).unwrap();
    let document: Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
    assert_eq!(document["query_history"], json!(["old", "B", "A"]));
    assert_eq!(document["unknown"]["keep"], true);
    fs::remove_dir_all(base).unwrap();
}

#[test]
fn tc_167_settings_rollback_failures_preserve_original_and_both_restore_errors() {
    let base = temp_dir("settings-rollback-errors");
    fs::create_dir_all(&base).unwrap();
    let path = base.join("ui-state.json");
    let roots = base.join("roots.txt");
    fs::write(&path, "{\"show_preview\":true}").unwrap();
    fs::write(&roots, "old-root\n").unwrap();
    let mut writes = Vec::new();
    let result = commit_settings_with_writer(
        &path,
        &[],
        false,
        Duration::from_secs(1),
        SettingsCommitRequest {
            request_id: 46,
            patch: UiStatePatch::from_json(json!({"show_preview": false})),
            saved_roots: Some((roots.clone(), "new-root\n".into())),
        },
        |target, bytes| {
            writes.push(target.to_path_buf());
            match writes.len() {
                1 => write_bytes_atomic(target, bytes),
                2 => crate::fs_atomic::write_bytes_atomic_with_sync_for_test(target, bytes, |_| {
                    Err(std::io::Error::other("injected durability failure"))
                }),
                3 => Err(std::io::Error::other("injected UI restore failure")),
                4 => Err(std::io::Error::other("injected roots restore failure")),
                _ => panic!("unexpected write"),
            }
        },
    );
    let error = result
        .err()
        .expect("rollback failures must not report success")
        .to_string();
    for message in [
        "injected durability failure",
        "UI-state rollback failed",
        "injected UI restore failure",
        "saved-roots rollback failed",
        "injected roots restore failure",
    ] {
        assert!(error.contains(message), "missing {message}: {error}");
    }
    assert_eq!(
        writes,
        vec![roots.clone(), path.clone(), path.clone(), roots.clone()]
    );
    let document: Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
    assert_eq!(document["show_preview"], false);
    assert_eq!(fs::read_to_string(&roots).unwrap(), "new-root\n");
    fs::remove_dir_all(base).unwrap();
}

#[test]
fn tc_167_post_replace_sync_failure_rolls_back_saved_roots() {
    let base = temp_dir("observed-settings-post-replace-roots");
    let ui_state_path = base.join("ui-state.json");
    let roots_path = base.join("roots.txt");
    fs::create_dir_all(&base).expect("create base");
    fs::write(
        &ui_state_path,
        json!({"default_root": "old-root"}).to_string(),
    )
    .expect("seed UI state");
    fs::write(&roots_path, "old-root\n").expect("seed roots");
    let before_ui = fs::read(&ui_state_path).expect("read prior UI state");
    let calls = std::sync::atomic::AtomicUsize::new(0);

    let result = commit_settings_with_writer(
        &ui_state_path,
        &[],
        false,
        Duration::from_secs(1),
        SettingsCommitRequest {
            request_id: 43,
            patch: UiStatePatch::from_json(json!({"default_root": "new-root"})),
            saved_roots: Some((roots_path.clone(), "new-root\n".to_string())),
        },
        |path, bytes| {
            if calls.fetch_add(1, std::sync::atomic::Ordering::SeqCst) == 0 {
                crate::fs_atomic::write_bytes_atomic_with_sync_for_test(path, bytes, |_| {
                    Err(std::io::Error::other("injected directory sync failure"))
                })
            } else {
                crate::fs_atomic::write_bytes_atomic(path, bytes)
            }
        },
    );

    let error = match result {
        Ok(_) => panic!("post-replace sync failure must be reported"),
        Err(error) => error,
    };
    assert!(error
        .to_string()
        .contains("destination was replaced but durability sync failed"));
    assert_eq!(fs::read(&ui_state_path).expect("read UI state"), before_ui);
    assert_eq!(
        fs::read_to_string(&roots_path).expect("read restored roots"),
        "old-root\n"
    );
    let _ = fs::remove_dir_all(&base);
}

#[test]
fn tc_167_post_replace_ui_state_sync_failure_rolls_back_both_files() {
    let base = temp_dir("observed-settings-post-replace-ui");
    let ui_state_path = base.join("ui-state.json");
    let roots_path = base.join("roots.txt");
    fs::create_dir_all(&base).expect("create base");
    fs::write(
        &ui_state_path,
        json!({"default_root": "old-root"}).to_string(),
    )
    .expect("seed UI state");
    fs::write(&roots_path, "old-root\n").expect("seed roots");
    let before_ui = fs::read(&ui_state_path).expect("read prior UI state");
    let calls = std::sync::atomic::AtomicUsize::new(0);

    let result = commit_settings_with_writer(
        &ui_state_path,
        &[],
        false,
        Duration::from_secs(1),
        SettingsCommitRequest {
            request_id: 44,
            patch: UiStatePatch::from_json(json!({"default_root": "new-root"})),
            saved_roots: Some((roots_path.clone(), "new-root\n".to_string())),
        },
        |path, bytes| {
            if calls.fetch_add(1, std::sync::atomic::Ordering::SeqCst) == 1 {
                crate::fs_atomic::write_bytes_atomic_with_sync_for_test(path, bytes, |_| {
                    Err(std::io::Error::other("injected directory sync failure"))
                })
            } else {
                crate::fs_atomic::write_bytes_atomic(path, bytes)
            }
        },
    );

    let error = match result {
        Ok(_) => panic!("post-replace sync failure must be reported"),
        Err(error) => error,
    };
    assert!(error
        .to_string()
        .contains("destination was replaced but durability sync failed"));
    assert_eq!(
        fs::read(&ui_state_path).expect("read restored UI state"),
        before_ui
    );
    assert_eq!(
        fs::read_to_string(&roots_path).expect("read restored roots"),
        "old-root\n"
    );
    let _ = fs::remove_dir_all(&base);
}

#[test]
fn tc_168_observed_settings_commit_enqueue_does_not_wait_for_ui_state_lock() {
    let base = temp_dir("observed-settings-frame-latency");
    let ui_state_path = base.join("ui-state.json");
    fs::create_dir_all(&base).expect("create base");
    let lock = acquire_sidecar_lock(&ui_state_path, Duration::from_millis(10)).expect("hold lock");

    let started = Instant::now();
    let response = enqueue_settings_commit(
        ui_state_path.clone(),
        false,
        SettingsCommitRequest {
            request_id: 42,
            patch: UiStatePatch::from_json(json!({"default_root": base})),
            saved_roots: None,
        },
    )
    .expect("enqueue observed commit");
    assert!(started.elapsed() < Duration::from_millis(200));

    drop(lock);
    let response = response
        .recv_timeout(Duration::from_secs(2))
        .expect("observed response");
    assert_eq!(response.request_id, 42);
    response.result.expect("commit after lock release");
    let _ = fs::remove_dir_all(&base);
}

#[test]
fn public_persisted_roots_and_history_api_honors_history_disabled() {
    let base = temp_dir("public-read-api");
    let ui_state_path = base.join("ui-state.json");
    let saved_roots_path = base.join("roots.txt");
    fs::create_dir_all(&base).expect("create base");
    fs::write(
        &ui_state_path,
        json!({
            "last_root": "C:/last",
            "default_root": "C:/default",
            "query_history": ["one", "two"]
        })
        .to_string(),
    )
    .expect("write ui state");
    fs::write(&saved_roots_path, "C:/saved\nC:/saved\nC:/other\n").expect("write roots");

    let enabled = crate::persistence::load_persisted_roots_and_history_from_paths(
        &ui_state_path,
        &saved_roots_path,
        false,
    );
    assert_eq!(enabled.query_history, vec!["one", "two"]);
    assert_eq!(enabled.saved_roots.len(), 2);
    assert_eq!(
        enabled.default_root,
        Some(normalize_windows_path_buf(PathBuf::from("C:/default")))
    );

    let disabled = crate::persistence::load_persisted_roots_and_history_from_paths(
        &ui_state_path,
        &saved_roots_path,
        true,
    );
    assert!(disabled.query_history.is_empty());
    assert_eq!(disabled.saved_roots, enabled.saved_roots);
    let _ = fs::remove_dir_all(&base);
}

#[test]
fn tc_167_latest_json_history_is_globally_normalized_before_unrelated_patch() {
    let base = temp_dir("latest-history-normalization");
    let path = base.join("ui-state.json");
    fs::create_dir_all(&base).expect("create base");
    fs::write(
        &path,
        json!({"query_history": [" first ", "second", "first", "", "third"]}).to_string(),
    )
    .expect("seed history");
    let writer = AsyncHistoryPersistence::new_with_lock_timeout(
        path.clone(),
        false,
        Duration::from_millis(50),
    );

    writer.enqueue_patch_for_test(
        UiStatePatch::from_json(json!({"show_preview": false})),
        Vec::new(),
    );
    writer.flush(Duration::from_secs(1)).expect("flush history");

    let written: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(&path).expect("read state")).expect("parse state");
    assert_eq!(
        written["query_history"],
        json!(["second", "first", "third"])
    );
    writer
        .shutdown(Duration::from_secs(1))
        .expect("shutdown writer");
    let _ = fs::remove_dir_all(&base);
}

#[test]
fn public_async_history_enqueue_is_a_noop_when_history_is_disabled() {
    let base = temp_dir("public-async-history-disabled");
    let path = base.join("ui-state.json");
    fs::create_dir_all(&base).expect("create base");
    fs::write(&path, json!({"query_history": ["existing"]}).to_string()).expect("seed history");
    let writer = crate::persistence::AsyncHistoryPersistence::new(path.clone(), true);

    writer
        .enqueue_history(vec!["ignored".into()])
        .expect("disabled enqueue");
    writer
        .flush(Duration::from_secs(1))
        .expect("disabled flush");

    let written: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(&path).expect("read state")).expect("parse state");
    assert_eq!(written["query_history"], json!(["existing"]));
    writer
        .shutdown(Duration::from_secs(1))
        .expect("shutdown writer");
    let _ = fs::remove_dir_all(&base);
}

#[test]
fn tc_167_seeded_gui_history_does_not_replay_over_latest_external_history() {
    let base = temp_dir("seeded-history");
    let path = base.join("ui-state.json");
    fs::create_dir_all(&base).expect("create base");
    fs::write(&path, json!({"query_history": ["local"]}).to_string()).expect("seed local");
    seed_persisted_history_snapshot(path.clone(), &["local".to_string()]);
    fs::write(
        &path,
        json!({"query_history": ["local", "external"]}).to_string(),
    )
    .expect("simulate external write");

    enqueue_ui_state_patch(
        path.clone(),
        UiStatePatch::from_json(json!({"show_preview": false})),
        vec!["local".into()],
        false,
    )
    .expect("admit patch");
    flush_ui_state_persistence(&path, Duration::from_secs(1)).expect("flush patch");

    let written: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(&path).expect("read state")).expect("parse state");
    assert_eq!(written["query_history"], json!(["local", "external"]));
    let _ = fs::remove_dir_all(&base);
}

#[test]
fn tc_167_generation_arriving_during_blocked_commit_is_flushed_next_without_loss() {
    let base = temp_dir("generation-during-commit");
    let path = base.join("ui-state.json");
    fs::create_dir_all(&base).expect("create base");
    let lock = acquire_sidecar_lock(&path, Duration::from_millis(10)).expect("hold lock");
    let writer = AsyncHistoryPersistence::new_with_lock_timeout(
        path.clone(),
        false,
        Duration::from_millis(80),
    );

    writer.enqueue_patch_for_test(
        UiStatePatch::from_json(json!({"first": 1})),
        vec!["A".into()],
    );
    std::thread::sleep(Duration::from_millis(15));
    writer.enqueue_patch_for_test(
        UiStatePatch::from_json(json!({"second": 2})),
        vec!["B".into()],
    );
    drop(lock);
    writer
        .flush(Duration::from_secs(1))
        .expect("flush generations");

    let written: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(&path).expect("read state")).expect("parse state");
    assert_eq!(written["first"], 1);
    assert_eq!(written["second"], 2);
    assert_eq!(written["query_history"], json!(["A", "B"]));
    writer
        .shutdown(Duration::from_secs(1))
        .expect("shutdown writer");
    let _ = fs::remove_dir_all(&base);
}

#[test]
fn tc_167_child_process_history_writer_helper() {
    let Ok(path) = env::var("FLISTWALKER_PERSISTENCE_CHILD_PATH") else {
        return;
    };
    let delta = env::var("FLISTWALKER_PERSISTENCE_CHILD_DELTA").expect("child delta");
    let writer = AsyncHistoryPersistence::new(PathBuf::from(path), false);
    writer.enqueue_history(vec![delta]).expect("child enqueue");
    writer.flush(Duration::from_secs(2)).expect("child flush");
    writer
        .shutdown(Duration::from_secs(2))
        .expect("child shutdown");
}

#[test]
fn tc_167_two_process_writers_preserve_alternating_history() {
    let base = temp_dir("two-process-writers");
    let path = base.join("ui-state.json");
    fs::create_dir_all(&base).expect("create base");
    let test_exe = env::current_exe().expect("current test executable");
    let helper = concat!(
        module_path!(),
        "::tc_167_child_process_history_writer_helper"
    );
    let helper = helper
        .strip_prefix("flist_walker::")
        .expect("crate-qualified helper");

    let parent_writer = AsyncHistoryPersistence::new(path.clone(), false);
    parent_writer
        .enqueue_history(vec!["A".into()])
        .expect("parent enqueue A");
    parent_writer
        .flush(Duration::from_secs(2))
        .expect("parent flush A");

    let output = Command::new(&test_exe)
        .arg("--exact")
        .arg(helper)
        .env("FLISTWALKER_PERSISTENCE_CHILD_PATH", &path)
        .env("FLISTWALKER_PERSISTENCE_CHILD_DELTA", "B")
        .output()
        .expect("run child writer");
    assert!(output.status.success(), "child writer B failed: {output:?}");
    assert!(
        String::from_utf8_lossy(&output.stdout).contains("1 passed"),
        "expected exactly one child helper test: {output:?}"
    );

    parent_writer
        .enqueue_history(vec!["A".into()])
        .expect("parent enqueue final A");
    parent_writer
        .flush(Duration::from_secs(2))
        .expect("parent flush final A");
    parent_writer
        .shutdown(Duration::from_secs(2))
        .expect("parent shutdown");

    let written: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(&path).expect("read state")).expect("parse state");
    assert_eq!(written["query_history"], json!(["B", "A"]));
    let _ = fs::remove_dir_all(&base);
}
